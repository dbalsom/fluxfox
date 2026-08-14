![image](../../doc/img/fluxfox_logo.png)

# fluxfox_cli: The FluxFox command-line utility

fluxfox_cli's functions are broken out by verbs.

## create

Create a recursively populated FAT12 disk image from a native directory:

```text
fluxfox_cli create --out_file disk.img --disk_format pc_360k --from_dir ./files
```

Directories read as `from_dir` sources are recursively searched by default.

Pass `--no_recursive` to copy only files in the source root.
Pass `--must_fit` to fail instead of returning a valid partially populated image when the disk fills.

ZIP archives can be used as the source with the same recursion and capacity behavior:

```text
fluxfox_cli create --out_file disk.img --disk_format pc_360k --from_archive ./files.zip
```

Directory and ZIP sources may contain one root-level `bootsector.bin` or `*_bootsector.bin`.
FluxFox installs it as sector 0 and patches its BPB for the selected disk format.
This file will not be copied into the FAT filesystem.

A boot-sector file at any path may also be supplied explicitly with `--bootsector`. It must be exactly 512 bytes and
takes precedence over a boot sector discovered in the source:

```text
fluxfox_cli create --out_file disk.img --disk_format pc_360k --from_dir ./files --bootsector ./dos/boot.bin
```

If the source root contains either of these two file pairs:
`IO.SYS` with `MSDOS.SYS`
`IBMIO.SYS` with `IBMDOS.SYS`
...then that pair of files is installed as the first two FAT entries. This permits the creation of a bootable DOS
diskette.
These files will be have the hidden, system and read-only DOS attributes set. A valid DOS boot sector must still be
provided - the default boot sector will not work.

