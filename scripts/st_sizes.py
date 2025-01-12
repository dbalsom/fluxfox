# FluxFox
# https://github.com/dbalsom/fluxfox
#
# Copyright 2024-2025 Daniel Balsom
#
# Permission is hereby granted, free of charge, to any person obtaining a
# copy of this software and associated documentation files (the “Software”),
# to deal in the Software without restriction, including without limitation
# the rights to use, copy, modify, merge, publish, distribute, sublicense,
# and/or sell copies of the Software, and to permit persons to whom the
# Software is furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in
# all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
# FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
# DEALINGS IN THE SOFTWARE.
#
# --------------------------------------------------------------------------

# Simple script to walk a directory path, looking in ZIP files for *.ST
# files, and collecting a histogram of their file sizes.
# In addition, we capture some parameters from the BIOS Parameter Block.

# This script showed me there is a lot more variation in ST raw sector
# image sizes than I expected. We'll need to use a more flexible approach
# in format determination for raw sector images than simply mapping a file
# size to a StandardFormat.

import os
import zipfile
import argparse
import csv
from collections import Counter
from struct import unpack

def collect_st_file_sizes_and_bpb(path):
    bpb_counter = Counter()

    for root, _, files in os.walk(path):
        for file in files:
            if file.lower().endswith('.zip'):
                zip_path = os.path.join(root, file)

                try:
                    with zipfile.ZipFile(zip_path, 'r') as zip_ref:
                        for file_info in zip_ref.infolist():
                            if file_info.filename.lower().endswith('.st'):
                                print(f"Found: {file_info.filename} Size: {file_info.file_size} bytes")
                                with zip_ref.open(file_info) as st_file:
                                    st_file.seek(11)  # Offset to the BPB structure
                                    bpb_data = st_file.read(25)  # Read the full BPB structure

                                    if len(bpb_data) < 25:
                                        print(f"Warning: {file_info.filename} has an incomplete BPB structure")
                                        continue

                                    bytes_per_sector, sectors_per_cluster, reserved_sectors, fats, root_entries, \
                                        total_sectors_16, media, sectors_per_fat_16, sectors_per_track, heads, \
                                        hidden_sectors, total_sectors_32 = unpack('<H B H B H H B H H H I I', bpb_data)

                                    if bytes_per_sector > 1024:
                                        print(f"Bad sector size: {bytes_per_sector}")
                                        continue

                                    total_sectors = total_sectors_16 if total_sectors_16 != 0 else total_sectors_32

                                    bpb_key = (file_info.file_size, bytes_per_sector, total_sectors, sectors_per_track, heads)
                                    bpb_counter[bpb_key] += 1

                except zipfile.BadZipFile:
                    print(f"Warning: Unable to read zip file {zip_path}")

    return bpb_counter

def save_to_csv(bpb_counter, output_file):
    with open(output_file, mode='w', newline='') as csvfile:
        csv_writer = csv.writer(csvfile)
        csv_writer.writerow(["File Size (bytes)", "Bytes/Sector", "Total Sectors", "Sectors/Track", "Heads", "Count"])
        sorted_bpb = sorted(bpb_counter.items(), key=lambda x: (x[0][0], x[0][3]), reverse=True)
        for (file_size, bytes_per_sector, total_sectors, sectors_per_track, heads), count in sorted_bpb:
            csv_writer.writerow([file_size, bytes_per_sector, total_sectors, sectors_per_track, heads, count])

def main():
    parser = argparse.ArgumentParser(description="Recursively find zip files and collect *.ST file sizes with BPB data.")
    parser.add_argument("path", type=str, help="Path to the directory to search")

    args = parser.parse_args()

    if not os.path.exists(args.path):
        print(f"Error: The path {args.path} does not exist.")
        return

    bpb_counter = collect_st_file_sizes_and_bpb(args.path)

    output_file = "st_info.csv"
    save_to_csv(bpb_counter, output_file)

    print(f"CSV file saved as {output_file}")

if __name__ == "__main__":
    main()
