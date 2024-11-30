![image](../../doc/img/fluxfox_logo.png)

# fluxfox imgviz

`imgviz`, is a command-line utility that can produce a visualization from a diskimage using fluxfox and save it to PNG.
fluxfox can produce a graphical visualization of a disk image as long as the image is of bitstream resolution or higher.

The following command will run `imgviz` and produce a 1024x1024 (or 2048x1024) resolution visualization with 4x
supersampling named `output.png`:

```
cargo run -r -p imgviz -- -i "input.pri" -o="output.png" --angle=2.88 --hole_ratio=0.66 --index_hole --data --metadata --decode --resolution=2048 --ss=4 
```

* The `angle` parameter determines the angle of the index mark on the unit circle for head #0. An angle of 0 is default
  and will place the index mark on the right side (the 3 o'clock position, similar to HxC)
* The `hole_ratio` parameter determines the relative size of the inner radius to the outer radius. A ratio of 0.66 is
  approximately accurate for a 5.25" diskette, mapping to 22mm of servo travel. You may prefer to reduce this factor for
  visualization purposes. HxC uses a value of 0.3 and Applesauce uses 0.27, for reference.
* `index_hole` will render a circle representing the position of the index hole on the diskette.
* `data` will render the data contained in the disk image, either as MFM-encoded or decoded stream, depending on whether
  `decode` is specified.
* `metadata` will overlay colored regions representing sector headers and sector data.
    * Either `data` or `metadata` must be supplied, or no image will be drawn!
* `decode` will decode the MFM-encoded data, showing a representation of the actual data on disk (usually more visually
  interesting)
* `resolution` determines the final output height of the resulting image. If an image is two-sided, it may be double
  this width or more.
* `ss` specifies a supersampling factor. The image will be rendered at this multiple of the specified `resolution` and
  down-sampled using the [fast_image_resize](https://github.com/Cykooz/fast_image_resize) crate.
* `errors` will render any decoding errors as the final layer on top of the visualization. This is useful for seeing the
  quality of the resolved image, spotting weak bits, etc.

If building from source, be sure to provide the `-r` parameter to cargo run, to run imgviz in release mode. Debug mode
will be very slow and use a lot more memory!

The image will be square with a single disk surface if the image is single-sided. Otherwise, both sides of the disk will
be rendered side by side.

When working with Kryoflux file sets, any file in a set may be used as an input filename.

Run with the `-h` parameter to see more command-line options.