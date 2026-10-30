// SPDX-License-Identifier: GPL-3.0-or-later
/** A feature this install can't offer, kept in view with the one-line reason why. */
export default function Unavailable({ reason, className = "" }: { reason: string; className?: string }) {
  return (
    <p className={`unavailable muted small ${className}`} role="note">
      {reason}
    </p>
  );
}
