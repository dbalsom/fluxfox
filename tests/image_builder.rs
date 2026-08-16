use fluxfox::{
    disk_lock::{NonTrackingDiskLock, NullContext},
    file_system::{fat::fat_fs::FatFileSystem, FileEntryAttributes, FileNameType, FileSystemError, FileSystemType},
    image_builder::ImageBuilder,
    prelude::*,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    ImageFormatParser,
    StandardFormat,
};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};

mod common;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

fn build_from_directory(path: &Path, recursive: bool, must_fit: bool) -> Result<DiskImage, DiskImageError> {
    ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_path(path, FileSystemType::Fat12, false, recursive, must_fit)
        .build()
}

fn build_from_archive(path: &Path, recursive: bool, must_fit: bool) -> Result<DiskImage, DiskImageError> {
    ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_archive_path(path, FileSystemType::Fat12, recursive, must_fit)
        .build()
}

fn build_from_archive_bytes(bytes: &[u8], recursive: bool, must_fit: bool) -> Result<DiskImage, DiskImageError> {
    ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_archive(bytes, FileSystemType::Fat12, recursive, must_fit)
        .build()
}

fn create_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();

    for (name, data) in entries {
        if let Some(data) = data {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        else {
            writer.add_directory(*name, options).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn create_zip(path: &Path, entries: &[(&str, Option<&[u8]>)]) {
    fs::write(path, create_zip_bytes(entries)).unwrap();
}

fn with_mounted_fat<T>(disk: DiskImage, callback: impl FnOnce(&FatFileSystem) -> T) -> T {
    let lock = NonTrackingDiskLock::new(disk.into_arc());
    let fs = FatFileSystem::mount(lock, NullContext::default(), Some(StandardFormat::PcFloppy360))
        .expect("FAT filesystem should mount");
    callback(&fs)
}

fn short_file_paths(fs: &FatFileSystem) -> Vec<String> {
    let tree = fs.build_file_tree_from_root().expect("filesystem tree should exist");
    tree.file_paths(true, FileNameType::Short)
        .into_iter()
        .map(|path| path.trim_start_matches('/').to_string())
        .collect()
}

fn boot_sector_bytes(disk: &DiskImage) -> Vec<u8> {
    disk.read_sector_basic(DiskCh::new(0, 0), DiskChsnQuery::new(0, 0, 1, 2), None)
        .expect("boot sector should be readable")
}

fn marked_boot_sector(marker: u8) -> Vec<u8> {
    let mut boot_sector = include_bytes!("../resources/bootsector.bin").to_vec();
    boot_sector[0x100] = marker;
    boot_sector
}

fn assert_boot_system_files(fs: &FatFileSystem, expected_names: [&str; 2]) {
    let root_entries = fs
        .build_file_tree_from_root()
        .expect("filesystem tree should exist")
        .dir("/")
        .expect("root directory should exist");
    let system_attributes = FileEntryAttributes::READ_ONLY | FileEntryAttributes::HIDDEN | FileEntryAttributes::SYSTEM;

    assert_eq!(root_entries[0].short_name(), expected_names[0]);
    assert_eq!(root_entries[1].short_name(), expected_names[1]);
    assert_eq!(root_entries[0].attributes(), system_attributes);
    assert_eq!(root_entries[1].attributes(), system_attributes);
}

#[test]
fn test_image_builder() {
    init();

    let mut image = match ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_creator_tag("MartyPC ".as_bytes())
        .with_filesystem(FileSystemType::Fat12)
        .build()
    {
        Ok(image) => image,
        Err(e) => panic!("Failed to create image: {}", e),
    };

    let mut out_buffer = Cursor::new(Vec::new());
    let output_fmt = DiskImageFileFormat::F86Image;
    match output_fmt.save_image(&mut image, &ParserWriteOptions::default(), &mut out_buffer) {
        Ok(_) => println!("Wrote 86F image."),
        Err(e) => panic!("Failed to write 86F image: {}", e),
    };

    assert!(!out_buffer.get_ref().is_empty());
}

#[test]
fn test_bundled_fox_boot_sector_is_the_default() {
    let disk = ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem(FileSystemType::Fat12)
        .build()
        .expect("default formatted image should build");
    let installed = boot_sector_bytes(&disk);
    let bundled = include_bytes!("../resources/bootsector.bin");

    // BPB bytes may be patched, while the bundled fox code and art remain intact.
    assert_eq!(&installed[0x40..], &bundled[0x40..]);
}

#[test]
fn test_explicit_boot_sector_is_installed_and_takes_precedence() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("bootsector.bin"), marked_boot_sector(0x11)).unwrap();
    fs::write(source.path().join("HELLO.TXT"), b"hello").unwrap();
    let explicit = marked_boot_sector(0x22);

    let disk = ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_path(source.path(), FileSystemType::Fat12, true, true, true)
        .with_bootsector(&explicit)
        .build()
        .expect("explicit boot sector should build");

    assert_eq!(boot_sector_bytes(&disk)[0x100], 0x22);
    with_mounted_fat(disk, |fs| {
        assert_eq!(short_file_paths(fs), vec!["HELLO.TXT"]);
    });
}

#[test]
fn test_boot_sector_is_discovered_from_directory() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("msdos_bootsector.bin"), marked_boot_sector(0x33)).unwrap();

    let disk = ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_path(source.path(), FileSystemType::Fat12, true, true, true)
        .build()
        .expect("directory boot sector should be discovered");

    assert_eq!(boot_sector_bytes(&disk)[0x100], 0x33);
    with_mounted_fat(disk, |fs| assert!(short_file_paths(fs).is_empty()));
}

#[test]
fn test_boot_sector_is_discovered_from_zip() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("boot.zip");
    let boot_sector = marked_boot_sector(0x44);
    create_zip(
        &archive,
        &[("BOOTSECTOR.BIN", Some(&boot_sector)), ("HELLO.TXT", Some(b"hello"))],
    );

    let disk = build_from_archive(&archive, true, true).expect("ZIP boot sector should be discovered");
    assert_eq!(boot_sector_bytes(&disk)[0x100], 0x44);
    with_mounted_fat(disk, |fs| {
        assert_eq!(short_file_paths(fs), vec!["HELLO.TXT"]);
    });
}

#[test]
fn test_invalid_or_ambiguous_boot_sectors_are_rejected() {
    let invalid = ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem(FileSystemType::Fat12)
        .with_bootsector(&[0; 511])
        .build();
    assert!(matches!(
        invalid,
        Err(DiskImageError::FilesystemError(FileSystemError::InvalidBootSector(_)))
    ));

    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("bootsector.bin"), marked_boot_sector(0x55)).unwrap();
    fs::write(source.path().join("dos_bootsector.bin"), marked_boot_sector(0x66)).unwrap();
    let ambiguous = build_from_directory(source.path(), true, true);
    assert!(matches!(
        ambiguous,
        Err(DiskImageError::FilesystemError(FileSystemError::InvalidBootSector(_)))
    ));
}

#[test]
fn test_bootable_directory_requires_custom_boot_sector() {
    let source = tempfile::tempdir().unwrap();
    let result = ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_filesystem_from_path(source.path(), FileSystemType::Fat12, true, true, true)
        .build();
    assert!(matches!(
        result,
        Err(DiskImageError::FilesystemError(FileSystemError::InvalidBootSector(_)))
    ));
}

#[test]
fn test_build_fat_from_directory_recursive() {
    init();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fs/example_filesystem");
    let disk = build_from_directory(&source, true, true).expect("recursive FAT image should build");

    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert!(paths.contains(&"HELLO.TXT".to_string()));
        assert!(paths.contains(&"LONG_F~1.TXT".to_string()));
        assert!(paths.contains(&"DIR_A/DIR_A_A/DIR_A_A.TXT".to_string()));
        assert!(paths.contains(&"DIR_C/DIR_C_A/DEEP/NESTED/MORE/HELLO.TXT".to_string()));
        assert!(!paths.iter().any(|path| path.contains("long_filename")));

        assert_eq!(
            fs.read_file("DIR_C/DIR_C_A/DEEP/NESTED/MORE/HELLO.TXT")
                .expect("deep file should be readable"),
            fs::read(source.join("dir_c/dir_c_a/deep/nested/more/hello.txt")).unwrap()
        );
        assert_eq!(
            fs.read_file("LONG_F~1.TXT").expect("short alias should be readable"),
            fs::read(source.join("long_filename.txt")).unwrap()
        );
    });
}

#[test]
fn test_build_fat_from_directory_shallow() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fs/example_filesystem");
    let disk = build_from_directory(&source, false, true).expect("shallow FAT image should build");

    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert_eq!(paths, vec!["HELLO.TXT", "LONG_F~1.TXT"]);
        let tree = fs.build_file_tree_from_root().unwrap();
        assert_eq!(tree.sub_dir_ct(), 0);
    });
}

#[test]
fn test_ms_dos_system_files_are_written_first_with_attributes() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("00_FIRST.TXT"), b"ordinary").unwrap();
    fs::write(source.path().join("io.sys"), b"io").unwrap();
    fs::write(source.path().join("msdos.sys"), b"dos").unwrap();

    let disk = build_from_directory(source.path(), true, true).expect("MS-DOS system pair should build");
    with_mounted_fat(disk, |fs| {
        assert_boot_system_files(fs, ["IO.SYS", "MSDOS.SYS"]);
        assert_eq!(fs.read_file("IO.SYS").unwrap(), b"io");
        assert_eq!(fs.read_file("MSDOS.SYS").unwrap(), b"dos");
    });
}

#[test]
fn test_incomplete_directory_system_file_pair_is_rejected() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("IO.SYS"), b"io").unwrap();

    let error = match build_from_directory(source.path(), true, true) {
        Ok(_) => panic!("incomplete pair should fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DiskImageError::FilesystemError(FileSystemError::InvalidBootFileSet(_))
    ));
}

#[test]
fn test_boot_system_pair_is_not_partially_written() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("IO.SYS"), vec![0x49; 200_000]).unwrap();
    fs::write(source.path().join("MSDOS.SYS"), vec![0x4D; 200_000]).unwrap();

    let error = match build_from_directory(source.path(), true, false) {
        Ok(_) => panic!("a system pair must not produce a partial image"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DiskImageError::FilesystemError(FileSystemError::NotEnoughSpace)
    ));
}

#[test]
fn test_build_fat_from_zip_recursive() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("files.zip");
    create_zip(
        &archive,
        &[
            ("hello.txt", Some(b"root")),
            ("long_filename.txt", Some(b"long")),
            ("dir_a/dir_b/deep.txt", Some(b"deep")),
            ("empty/", None),
        ],
    );

    let disk = build_from_archive(&archive, true, true).expect("recursive ZIP image should build");
    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert!(paths.contains(&"HELLO.TXT".to_string()));
        assert!(paths.contains(&"LONG_F~1.TXT".to_string()));
        assert!(paths.contains(&"DIR_A/DIR_B/DEEP.TXT".to_string()));
        assert_eq!(fs.read_file("HELLO.TXT").unwrap(), b"root");
        assert_eq!(fs.read_file("LONG_F~1.TXT").unwrap(), b"long");
        assert_eq!(fs.read_file("DIR_A/DIR_B/DEEP.TXT").unwrap(), b"deep");

        let tree = fs.build_file_tree_from_root().unwrap();
        assert!(tree.node("EMPTY").is_some(), "empty ZIP directories should be retained");
    });
}

#[test]
fn test_build_fat_from_in_memory_zip_recursive() {
    let boot_sector = marked_boot_sector(0x7A);
    let archive = create_zip_bytes(&[
        ("bootsector.bin", Some(&boot_sector)),
        ("hello.txt", Some(b"root")),
        ("dir_a/dir_b/deep.txt", Some(b"deep")),
    ]);

    let disk = build_from_archive_bytes(&archive, true, true).expect("in-memory ZIP image should build");
    assert_eq!(boot_sector_bytes(&disk)[0x100], 0x7A);
    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert_eq!(paths, vec!["DIR_A/DIR_B/DEEP.TXT", "HELLO.TXT"]);
        assert_eq!(fs.read_file("HELLO.TXT").unwrap(), b"root");
        assert_eq!(fs.read_file("DIR_A/DIR_B/DEEP.TXT").unwrap(), b"deep");
    });
}

#[test]
fn test_build_fat_from_zip_shallow() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("files.zip");
    create_zip(
        &archive,
        &[
            ("hello.txt", Some(b"root")),
            ("long_filename.txt", Some(b"long")),
            ("dir_a/deep.txt", Some(b"deep")),
            ("empty/", None),
        ],
    );

    let disk = build_from_archive(&archive, false, true).expect("shallow ZIP image should build");
    with_mounted_fat(disk, |fs| {
        assert_eq!(short_file_paths(fs), vec!["HELLO.TXT", "LONG_F~1.TXT"]);
        assert_eq!(fs.build_file_tree_from_root().unwrap().sub_dir_ct(), 0);
    });
}

#[test]
fn test_ibm_dos_system_files_from_zip_are_written_first_with_attributes() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("system.zip");
    create_zip(
        &archive,
        &[
            ("00_FIRST.TXT", Some(b"ordinary")),
            ("IBMDOS.SYS", Some(b"dos")),
            ("IBMIO.SYS", Some(b"io")),
        ],
    );

    let disk = build_from_archive(&archive, true, true).expect("IBM DOS system pair should build");
    with_mounted_fat(disk, |fs| {
        assert_boot_system_files(fs, ["IBMIO.SYS", "IBMDOS.SYS"]);
        assert_eq!(fs.read_file("IBMIO.SYS").unwrap(), b"io");
        assert_eq!(fs.read_file("IBMDOS.SYS").unwrap(), b"dos");
    });
}

#[test]
fn test_mixed_zip_system_file_pairs_are_rejected() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("mixed.zip");
    create_zip(
        &archive,
        &[
            ("IO.SYS", Some(b"io")),
            ("MSDOS.SYS", Some(b"dos")),
            ("IBMIO.SYS", Some(b"ibm")),
        ],
    );

    let error = match build_from_archive(&archive, true, true) {
        Ok(_) => panic!("mixed system families should fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DiskImageError::FilesystemError(FileSystemError::InvalidBootFileSet(_))
    ));
}

#[test]
fn test_zip_must_fit_rejects_oversized_archive() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("large.zip");
    let data = vec![0xA5; 400_000];
    create_zip(&archive, &[("TOO_BIG.BIN", Some(&data))]);

    let error = match build_from_archive(&archive, true, true) {
        Ok(_) => panic!("oversized ZIP should fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DiskImageError::FilesystemError(FileSystemError::NotEnoughSpace)
    ));
}

#[test]
fn test_non_must_fit_returns_clean_partial_image_from_zip() {
    let source = tempfile::tempdir().unwrap();
    let archive = source.path().join("large.zip");
    let data = vec![0x5A; 400_000];
    create_zip(
        &archive,
        &[
            ("00_FIRST.TXT", Some(b"first")),
            ("01_HUGE.BIN", Some(&data)),
            ("02_LATER.TXT", Some(b"later")),
        ],
    );

    let disk = build_from_archive(&archive, true, false).expect("partial ZIP image should be returned");
    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert_eq!(paths, vec!["00_FIRST.TXT"]);
        assert_eq!(fs.read_file("00_FIRST.TXT").unwrap(), b"first");
        assert!(!paths.iter().any(|path| path.contains("HUGE") || path.contains("LATER")));
    });
}

#[test]
fn test_long_directory_and_colliding_names_without_lfn() {
    let source = tempfile::tempdir().unwrap();
    let long_dir = source.path().join("directory_name_longer_than_8_chars");
    fs::create_dir(&long_dir).unwrap();
    fs::create_dir(source.path().join("EMPTY")).unwrap();
    fs::write(long_dir.join("collision_alpha_filename.txt"), b"alpha").unwrap();
    fs::write(long_dir.join("collision_beta_filename.txt"), b"beta").unwrap();

    let disk = build_from_directory(source.path(), true, true).expect("long names should be aliased");
    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.contains('~')));
        assert!(!paths.iter().any(|path| path.contains("directory_name")));
        assert!(!paths.iter().any(|path| path.contains("collision_")));

        let mut contents = paths.iter().map(|path| fs.read_file(path).unwrap()).collect::<Vec<_>>();
        contents.sort();
        assert_eq!(contents, vec![b"alpha".to_vec(), b"beta".to_vec()]);

        let tree = fs.build_file_tree_from_root().unwrap();
        assert!(tree.node("EMPTY").is_some(), "empty directories should be retained");
    });
}

#[test]
fn test_must_fit_rejects_oversized_directory() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("TOO_BIG.BIN"), vec![0xA5; 400_000]).unwrap();

    let error = match build_from_directory(source.path(), true, true) {
        Ok(_) => panic!("oversized source should fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DiskImageError::FilesystemError(FileSystemError::NotEnoughSpace)
    ));
}

#[test]
fn test_non_must_fit_returns_clean_partial_image() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("00_FIRST.TXT"), b"first").unwrap();
    fs::write(source.path().join("01_HUGE.BIN"), vec![0x5A; 400_000]).unwrap();
    fs::write(source.path().join("02_LATER.TXT"), b"later").unwrap();

    let disk = build_from_directory(source.path(), true, false).expect("partial image should be returned");
    with_mounted_fat(disk, |fs| {
        let paths = short_file_paths(fs);
        assert_eq!(paths, vec!["00_FIRST.TXT"]);
        assert_eq!(fs.read_file("00_FIRST.TXT").unwrap(), b"first");
        assert!(!paths.iter().any(|path| path.starts_with("FF") || path.contains("HUGE")));
    });
}

#[test]
fn test_directory_image_survives_raw_serialization() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fs/example_filesystem");
    let mut disk = build_from_directory(&source, true, true).unwrap();
    let mut image = Cursor::new(Vec::new());
    DiskImageFileFormat::RawSectorImage
        .save_image(&mut disk, &ParserWriteOptions::default(), &mut image)
        .unwrap();
    image.set_position(0);

    let reloaded = DiskImage::load(&mut image, None, None, None).expect("raw image should reload");
    with_mounted_fat(reloaded, |fs| {
        assert_eq!(
            fs.read_file("HELLO.TXT").unwrap(),
            fs::read(source.join("hello.txt")).unwrap()
        );
        assert!(short_file_paths(fs).contains(&"LONG_F~1.TXT".to_string()));
    });
}
