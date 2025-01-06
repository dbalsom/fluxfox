# fluxfox_svg

A helper library for generating disk image visualizations as SVG using fluxfox's native visualization functions.

This library uses the [svg](https://crates.io/crates/svg) crate to construct SVGs programmatically.
If you wish to render the resulting SVG, you may wish to look into [resvg](https://crates.io/crates/resvg), however
rendering fluxfox's display lists directly with `tiny_skia` may be more efficient as `resvg` ends up rasterizing with
`tiny_skia` anyway.

See the `fluxfox_tiny_skia` crate in this same repository.

## Features

- `serde`: This feature will derive `Serialize` and `Deserialize` for fluxfox_svg's helper types such as `BlendMode`
  and `ElementStyle` that you might want to expose via command line arguments or other UI.

  It does not implement deserialization for the `SvgRenderer` or any temporary related types.

