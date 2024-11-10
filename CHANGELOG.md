## 0.2.0 (2024-11-XX)

### Features:

- Added support for WEAK chunk in PRI images.
- Added support for PFI (PCE Flux Image) images
- Added support for visualization of bitstream errors
- Added offset fields to track interface functions to support tracks with duplicate sector IDs
- Implemented DiskChsnQuery struct to enable optional matching of Sector ID fields when scanning, reading, or writing
  a sector.

### Bugfixes:

- Fixed bugs with 86F import when not using track absolute bitcell counts
- Fixed and improved format tests
- Fixed bug in Kryoflux import

### Breaking changes:

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