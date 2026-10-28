// SPDX-License-Identifier: GPL-3.0-or-later
//! Public-web transport for untrusted shared selections. The resolver returns the exact
//! checked socket addresses consumed by TCP: no second DNS lookup or proxy can bypass it.
use std::net::IpAddr;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

#[derive(Debug, Default)]
struct PublicResolver<R = DefaultResolver>(R);
impl<R: Resolver> Resolver for PublicResolver<R> {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri.authority().is_none_or(|a| a.as_str().contains('@'))
        {
            return Err(ureq::Error::HostNotFound);
        }
        let addresses = self.0.resolve(uri, config, timeout)?;
        // Reject mixed answers too, rather than accepting a DNS rebinding candidate.
        if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addresses)
    }
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 192 && b == 0)
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v) => {
            // Permit global unicast only; deny special-use/documentation and IPv4
            // translation/tunnelling ranges rather than trusting their embedded address.
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}
pub(crate) fn agent(https_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(std::time::Duration::from_secs(25)))
        .https_only(https_only)
        .user_agent(crate::fetch::BROWSER_AGENT)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::with_parts(
        config,
        DefaultConnector::new(),
        PublicResolver::<DefaultResolver>::default(),
    )
}
/// Anonymous pages and assets use the same checked transport as signed-in clients.
pub(crate) struct PublicHttp;
impl crate::Net for PublicHttp {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        crate::fetch::read(agent(false).get(url).call())
    }
    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        crate::fetch::read(
            agent(false)
                .post(url)
                .header(
                    "Content-Type",
                    "application/x-www-form-urlencoded;charset=UTF-8",
                )
                .send(body),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug)]
    struct Addresses(Vec<std::net::SocketAddr>);
    impl Resolver for Addresses {
        fn resolve(
            &self,
            _: &ureq::http::Uri,
            _: &ureq::config::Config,
            _: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, ureq::Error> {
            let mut out = self.empty();
            for a in &self.0 {
                out.push(*a);
            }
            Ok(out)
        }
    }
    #[test]
    fn public_web_rejects_local_literal_and_dns_destinations_before_connecting() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let local = listener.local_addr().unwrap();
        for addresses in [
            vec![local],
            vec!["93.184.216.34:80".parse().unwrap(), local],
        ] {
            let config = agent(false).config().clone();
            assert!(config.proxy().is_none());
            let net = ureq::Agent::with_parts(
                config,
                DefaultConnector::new(),
                PublicResolver(Addresses(addresses)),
            );
            assert!(net.get("http://public.example/article").call().is_err());
        }
        assert!(
            agent(false)
                .get(format!("http://{local}/article"))
                .call()
                .is_err()
        );
        assert!(listener.accept().is_err());
    }
    #[derive(Debug)]
    struct RedirectConnector(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    #[derive(Debug)]
    struct RedirectTransport {
        buffers: ureq::unversioned::transport::LazyBuffers,
        response: Vec<u8>,
    }
    impl ureq::unversioned::transport::Transport for RedirectTransport {
        fn buffers(&mut self) -> &mut dyn ureq::unversioned::transport::Buffers {
            &mut self.buffers
        }
        fn transmit_output(&mut self, _: usize, _: NextTimeout) -> Result<(), ureq::Error> {
            Ok(())
        }
        fn await_input(&mut self, _: NextTimeout) -> Result<bool, ureq::Error> {
            use ureq::unversioned::transport::Buffers;
            let n = self
                .response
                .len()
                .min(self.buffers.input_append_buf().len());
            self.buffers.input_append_buf()[..n].copy_from_slice(&self.response[..n]);
            self.buffers.input_appended(n);
            self.response.drain(..n);
            Ok(n > 0)
        }
        fn is_open(&mut self) -> bool {
            false
        }
    }
    impl ureq::unversioned::transport::Connector for RedirectConnector {
        type Out = RedirectTransport;
        fn connect(
            &self,
            _: &ureq::unversioned::transport::ConnectionDetails,
            _: Option<()>,
        ) -> Result<Option<Self::Out>, ureq::Error> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Some(RedirectTransport { buffers: ureq::unversioned::transport::LazyBuffers::new(4096,4096),
                response: b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec() }))
        }
    }
    #[derive(Debug)]
    struct RedirectResolver(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
    impl Resolver for RedirectResolver {
        fn resolve(
            &self,
            uri: &ureq::http::Uri,
            _: &ureq::config::Config,
            _: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, ureq::Error> {
            self.0
                .lock()
                .unwrap()
                .push(uri.host().unwrap_or_default().to_owned());
            let mut out = self.empty();
            out.push(
                if uri.host() == Some("public.example") {
                    "93.184.216.34:80"
                } else {
                    "127.0.0.1:80"
                }
                .parse()
                .unwrap(),
            );
            Ok(out)
        }
    }
    #[test]
    fn public_web_redirect_rechecks_actual_destination_before_second_connection() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let resolutions = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let net = ureq::Agent::with_parts(
            agent(false).config().clone(),
            RedirectConnector(calls.clone()),
            PublicResolver(RedirectResolver(resolutions.clone())),
        );
        assert!(net.get("http://public.example/article").call().is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            *resolutions.lock().unwrap(),
            ["public.example", "127.0.0.1"]
        );
    }
    #[test]
    fn public_web_special_ranges_are_denied() {
        for ip in [
            "0.0.0.0",
            "10.1.2.3",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002:7f00:1::",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
