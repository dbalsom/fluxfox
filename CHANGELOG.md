## 0.2.0 (2025-01-15)

### New Features:

#### Extensively refactored. Not all new features may be listed as I have lost track!

- Track read() no longer requires mutable reference
- Revamped visualization functions to add vectorization
- Added the "Source Map" - a tree/hash table that captures the structure of a source disk image file format.
    - This is useful for research or debugging of file formats.
- Added an `ImageLoader` which is the preferred interface for loading a disk image from a disk image file.
- Added a `Platform` enumeration that defines the computing platform a disk image is intended for
    - Added Amiga, Atari ST, Macintosh and Apple II platforms
- Added a `TrackSchema` enumeration that defines the specific format of a disk track
- Added `ParserReadOptions` and `ParserWriteOptions` parameters to `ImageFileParser` to eventually better control
  disk image reading and writing.
- Added a convenience wrapper, `StandardFormatParam`, for use with parsing user input into `StandardFormat`s
- Added silly visualization functions to map a Pixmap to a `DiskImage`. See the png2disk crate for usage.
- Added `SectorLayout` struct to define a standard sector layout.
    - This can have an adjustable sector id offset to support PC vs Amiga sector layouts (Amiga sectors start at 0).
    - Added `DiskCh`, `DiskChs` and `DiskChsn` iterators for walking through sector layouts
        - `SectorLayout::ch_iter()`
        - `SectorLayout::chs_iter()`
        - `SectorLayout::chsn_iter()`
- Added a `StandardSectorView` interface wrapper that provides `Read` + `Write` + `Seek` traits for a `DiskImage`
    - This provides a logical sector view over a disk image, as if it were a raw sector image.
    - This feature allows interfacing with library crates that expect a raw sector image, such as `rust-fatfs`.
- Added basic FAT support, based on `rust-fatfs`, and example

### Disk Image Format updates:

- New disk image file parsers - IPF, MOOF, WOZ
- Added progress reporting for MFI loader.
- Added support for high density MFI images.
- Added support for WEAK chunk in PRI images.
- Added support for PFI (PCE Flux Image) images
- Added support for visualization of bitstream errors
- Added offset fields to track interface functions to support tracks with duplicate sector IDs
- Implemented `DiskChsnQuery` struct to enable optional matching of Sector ID fields when scanning, reading, or writing
  a sector.
    - Type aliased to `SectorIdQuery`
- Added several feature flags to fine-tune desired fluxfox functionality and dependencies

### Bugfixes:

- Fixed bugs in Bios Parameter Block defaults for 1.2MB images.
- Fixed several bugs in MFI import.
- Refactored IMG export to use a standard DOS view of sectors
- Fix extreme memory usage when decoding flux tracks
- Create empty MFM and FM tracks with valid clock bits
- Fixed and improved format tests
- Fixed bug in Kryoflux import

### Breaking changes:

#### This list is getting a bit long... basically, the library has been heavily refactored. Just rewrite everything :)

- Error flags have been normalized so that aberrant conditions are positive states:
    - `address_crc_valid` renamed to `address_error` in several contexts
    - `data_crc_valid` renamed to `data_error` in several contexts
    - This allows construction of 'normal' data structures using Default in a consistent way.
- `ReadSectorResult` now contains a `DataIntegrity` struct instead of `SectorCrc` to support schemas that use different
  integrity checks (CRC vs checksum)
- Removed `default_sector_size` from `DiskDescriptor`.
- `add_track_bitstream` now takes a `BitStreamTrackParams` reference.
- `add_track_fluxstream` now takes a `FluxStreamTrackParams` reference.
- `add_track_metasector` now takes a `MetaSectorTrackParams` reference.
- `load_image` and `save_image` methods of `ImageFileParser` now take a `ParserReadOptions` and `ParserWriteOptions`
  reference, respectively.
- `ImageWriter` now takes a reference to `DiskImage` in `new` instead of `write`.
- `track_stream` and `track_stream_mut` methods of `Track` were renamed to `stream` and `stream_mut`.
- `read_track` and `read_track_raw` methods of `Track` were renamed to `read` and `read_raw`.
- Renamed `DiskConsistency` to `DiskAnalysis`, and `TrackConsistency` to `TrackAnalysis`.
    - "Consistency" was a somewhat imprecise and confusing term.
    - `track_consistency` was renamed to `analysis`
- Renamed many 'Disk*' enums to 'Track*' to be more clear that these parameters may vary per track.
    - `DiskDensity` -> `TrackDensity`
    - `DiskDataEncoding` -> `TrackDataEncoding`
    - `DiskDataRate` -> `TrackDataRate`
- `BiosParameterBlock2` and `BiosParameterBlock2` now implement `try_from<StandardFormat>` instead of
  `from<StandardFormat>` to support non-pc `StandardFormat`s
- `read_sector()` method of `DiskImage` and `Track` no longer requires a mutable reference.
- Several changes to `DiskCh*` structs and trait implementations.
- Moved common type imports to `fluxfox::prelude`
- Removed `get_*` prefixes from API per Rust style guidelines.
- Removed the `Invalid` variant of `StandardFormat`. Replaced `From<usize>` with`TryFrom<usize>`
- All `add_track_*` API functions now return a mutable reference to the new `Track` on success
- Track API changes
    - Implemented `DiskChsnQuery` struct for optional matching of Sector ID fields
    - Added an optional bit offset field to track `read_sector`, `scan_sector` and `write_sector` methods to support
      tracks with duplicate sector IDs
- Factored common sector attributes into `SectorAttributes` struct. Affects `SectorDescriptor` and `SectorMapEntry`.
- Renamed `RwSectorScope::DataBlock` to `RwSectorScope::DataElement` for internal naming consistency
- Changed `DiskCh::seek_next_track()` to take a mutable reference to self.
- Replaced `render_track_weak_bits()` with `render_track_map()` which can render different kinds of track masks
  defined by `RenderMapType`

## 0.1.0 (2024-11-07)

Initial release of fluxfox. Start of versioning.