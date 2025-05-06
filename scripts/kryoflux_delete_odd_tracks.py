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

# Script to delete odd tracks from a Kryoflux raw stream set and rename
# the files appropriately.


import os
import re
import argparse
import sys

FILE_REX = re.compile(r".*(\d{2})\.(\d)\.raw")

def delete_odd_tracks(directory_path):
    """Deletes files with odd track numbers from the directory."""
    for filename in os.listdir(directory_path):
        match = FILE_REX.match(filename)
        if match:
            track_value = int(match.group(1))
            head_value = int(match.group(2))

            # Validate that head_value is 0 or 1
            if head_value not in {0, 1}:
                print(f"Error: Unexpected head value '{head_value}' in file '{filename}'")
                sys.exit(1)

            # Delete files with odd capture group matches
            if track_value % 2 != 0:
                file_path = os.path.join(directory_path, filename)
                os.remove(file_path)
                print(f"Deleted '{filename}' (capture group: {track_value}, subset: {head_value})")


def renumber_even_tracks(directory_path):
    """Renumbers files with even track numbers sequentially, within each subset, starting from 0."""
    even_files = {0: [], 1: []}

    # Collect files with even track matches into head-specific lists
    for filename in os.listdir(directory_path):
        match = FILE_REX.match(filename)
        if match:
            track_value = int(match.group(1))
            head_value = int(match.group(2))

            # Validate that head_value is 0 or 1
            if head_value not in {0, 1}:
                print(f"Error: Unexpected head value '{head_value}' in file '{filename}'")
                sys.exit(1)

            # Only include files with even tracks
            if track_value % 2 == 0:
                even_files[head_value].append((filename, track_value))
            else:
                print(f"Error: Unexpected odd track value '{track_value}' in file '{filename}'")
                sys.exit(1)

    # Renumber each head list separately
    for head_value, files in even_files.items():
        # Sort files by capture group match value within each subset
        files.sort(key=lambda x: x[1])

        # Renumber files sequentially within this subset
        for i, (filename, _) in enumerate(files):
            new_filename = re.sub(r"(\d{2})", f"{i:02}", filename, count=1)

            if filename == new_filename:
                continue

            old_path = os.path.join(directory_path, filename)
            new_path = os.path.join(directory_path, new_filename)

            # Rename the file
            os.rename(old_path, new_path)
            print(f"Renamed '{filename}' to '{new_filename}' in subset {head_value}")


def main():
    parser = argparse.ArgumentParser(description="Process and renumber files in a directory.")
    parser.add_argument("directory", help="Path to the directory containing the files.")
    args = parser.parse_args()

    # Step 1: Delete all files with odd track numbers
    delete_odd_tracks(args.directory)

    # Step 2: Renumber even-matching files sequentially within each head group
    renumber_even_tracks(args.directory)


if __name__ == "__main__":
    main()
