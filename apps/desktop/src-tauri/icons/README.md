# App icons

Deckle's icon is the morning paper: a sheet with a deckle edge, a Fraktur masthead "D" over the
accent rule, columns of type and the 5×5 crossword, on a warm board.

- `Deckle.icon/`: the Icon Composer (Liquid Glass) source, with the artwork as SVG layers in
  `Assets/`. Open it in Icon Composer to view or adjust the glass.
- Everything else (`icon.icns`, `icon.ico`, the PNGs and the Windows Store logos) is a flat render
  of the same artwork on Apple's macOS icon grid, made with `pnpm tauri icon` from a 1024 px PNG.

On macOS the release workflow compiles `Deckle.icon` with Xcode 26's `actool` into
`Resources/Assets.car` and sets `CFBundleIconName` (`../icon.macos.conf.json`,
`../icon.macos.Info.plist`), so macOS 26 shows the Liquid Glass icon. `icon.icns` stays as the
`CFBundleIconFile` fallback. Local `pnpm tauri build` doesn't run that step and ships the flat icon.
