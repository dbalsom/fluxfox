# FluxFox
# https://github.com/dbalsom/fluxfox
#
# Copyright 2024 Daniel Balsom
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

# This script showed me there is a lot more variation in ST raw sector
# image sizes than I expected. We'll need to use a more flexible approach
# in format determination for raw sector images than simply mapping a file
# size to a StandardFormat.

import os
import zipfile
import argparse
from collections import Counter

def collect_st_file_sizes(path):
    file_size_counter = Counter()

    for root, _, files in os.walk(path):
        for file in files:
            if file.lower().endswith('.zip'):
                zip_path = os.path.join(root, file)

                try:
                    with zipfile.ZipFile(zip_path, 'r') as zip_ref:
                        for file_info in zip_ref.infolist():
                            if file_info.filename.lower().endswith('.st'):
                                print(f"Found: {file_info.filename} Size: {file_info.file_size} bytes")
                                file_size_counter[file_info.file_size] += 1
                except zipfile.BadZipFile:
                    print(f"Warning: Unable to read zip file {zip_path}")

    return file_size_counter

def main():
    parser = argparse.ArgumentParser(description="Recursively find zip files and collect *.ST file sizes.")
    parser.add_argument("path", type=str, help="Path to the directory to search")

    args = parser.parse_args()

    if not os.path.exists(args.path):
        print(f"Error: Path {args.path} not found.")
        return

    file_size_counter = collect_st_file_sizes(args.path)

    print("Unique file sizes of *.ST files:")
    for size, count in file_size_counter.most_common():
        print(f"Size: {size} bytes, Count: {count}")

if __name__ == "__main__":
    main()
