## 0.1.0 (2024-11-XX)

### Features:

- Added support for visualization of bitstream errors

### Bugfixes:

- Fixed bug in Kryoflux import

### Breaking changes:

- Renamed RwSectorScope::DataBlock to RwSectorScope::DataElement
- Changed DiskCh::seek_next_track() to take a mutable reference to self.
- Renamed render_track_weak_bits() to render_track_map() which can render track maps
  defined by RenderMapType

### Dependency updates:

- thiserror = "2.0"
- regex = "1.11"

## 0.1.0 (2024-11-07)

Initial release of fluxfox. Start of versioning.