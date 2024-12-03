![image](../../doc/img/fluxfox_logo.png)

# fat_example

`fat_example` demonstrates reading a FAT filesystem from a fluxfox `DiskImage`.

The basic logic for doing so is quite simple:

```
    let disk_arc = DiskImage::into_arc(disk);

    // Mount the filesystem
    let fs = match FatFileSystem::mount(disk_arc.clone(), None) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error mounting filesystem: {}", e);
            std::process::exit(1);
        }
    };

    // Get file listing.
    let files = fs.list_all_files();

    for file in files {
        println!("{}", file);
    }
```

The `DiskImage` is converted into an `Arc<RwLock<DiskImage>>` with the convenience function `into_arc`,
which is cloned and passed to `FatFileSystem::mount`.

We then call `FatFileSystem::list_all_files`, which returns a vector of short DOS filenames as `String`.

The `FatFileSystem` will be invalidated if a track is reformatted, the disk's sector layout is otherwise modified, or
the sectors containing the boot sector or FAT tables are modified. It should be dropped and recreated in that event.

The `FatFileSystem` interface is subject to change as it is in active development. Eventually a `FileSystem` trait will
be defined to support different filesystems.

