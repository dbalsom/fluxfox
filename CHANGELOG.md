## 0.2.0 (2024-11-XX)

### Features:

- Added a convenience wrapper, StandardFormatParam, for use with parsing user input into StandardFormat
- Added silly visualization functions to map a Pixmap to a DiskImage. See the png2disk crate for usage.
- Added DiskCh, DiskChs and DiskChsn iterators for walking geometries of a StandardFormat
- Added a StandardSectorView interface that provides Read + Write + Seek traits for a DiskImage
- Added basic FAT support and example
- Added progress reporting for MFI loader.
- Added support for WEAK chunk in PRI images.
- Added support for PFI (PCE Flux Image) images
- Added support for visualization of bitstream errors
- Added offset fields to track interface functions to support tracks with duplicate sector IDs
- Implemented DiskChsnQuery struct to enable optional matching of Sector ID fields when scanning, reading, or writing
  a sector.
    - Type aliased to SectorIdQuery

### Bugfixes:

- Fixed bugs in Bios Parameter Block defaults for 1.2MB images.
- Fixed several bugs in MFI import.
- Refactored IMG export to use a standard DOS view of sectors
- Fix extreme memory usage when decoding flux tracks
- Create empty MFM and FM tracks with valid clock bits
- Fixed and improved format tests
- Fixed bug in Kryoflux import

### Breaking changes:

- read_sector operations no longer require a mutable reference.
- Several changes to DiskCh* structs and trait implementations.
- Moved common type imports to fluxfox::prelude
- Removed get_* prefixes from API per Rust style guidelines.
- Removed Invalid variant of StandardFormat. Removed From<> and implemented TryFrom<usize>
- All add_track_* API functions now return a mutable reference to the new track on success
- Track API changes
    - Implemented DiskChsnQuery struct for optional matching of Sector ID fields
    - Added bit offset field to track sector read and write functions to support tracks with duplicate sector IDs
- Factored common sector attributes into SectorAttributes struct. Affects SectorDescriptor and SectorMapEntry.
- Renamed RwSectorScope::DataBlock to RwSectorScope::DataElement
- Changed DiskCh::seek_next_track() to take a mutable reference to self.
- Renamed render_track_weak_bits() to render_track_map() which can render track maps
  defined by RenderMapType

### Dependency updates:

- thiserror = "2.0"
- regex = "1.11"

## 0.1.0 (2024-11-07)

Initial release of fluxfox. Start of versioning.