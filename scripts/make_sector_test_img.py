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

# A simple script to crate a test image file from the specified number of sectors.

# The sector count should be
#  - 160K - 320
#  - 180K - 360
#  - 320K - 640
#  - 360K - 720
#  - 720K - 1440
#  - 1.2M - 2400
#  - 1.44M - 2880
#  - 2.88M - 5760

import sys


def create_img_file(sector_count):
    with open('sector_test_360k.img', 'wb') as f:
        byte_value = 0
        for _ in range(sector_count):
            f.write(bytes([byte_value] * 512))
            byte_value = (byte_value + 1) % 256


def main():
    if len(sys.argv) != 2:
        print("Usage: python script.py <sector_count>")
        sys.exit(1)

    try:
        sector_count = int(sys.argv[1])
    except ValueError:
        print("Error: Sector count must be an integer.")
        sys.exit(1)

    create_img_file(sector_count)


if __name__ == "__main__":
    main()

