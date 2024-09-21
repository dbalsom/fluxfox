/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------

    examples/imgconvert/src/prompt.rs

    Implement a simple prompt that requires the user to enter 'y' or 'n'.
*/
use std::io;
use std::io::Write;

pub(crate) fn prompt(message: &str) -> bool {
    loop {
        // Display the prompt message without a newline at the end
        print!("{}", message);
        // Ensure the prompt is shown immediately
        io::stdout().flush().expect("Failed to flush stdout");

        // Read the user's input
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read line");

        // Process the input: trim whitespace and convert to lowercase
        let input = input.trim().to_lowercase();

        // Match the input against accepted responses
        match input.as_str() {
            "y" | "yes" => return true, // User answered 'yes'
            "n" | "no" => return false, // User answered 'no'
            _ => {
                // Invalid input, prompt again
                println!("Please enter 'y' or 'n'.");
            }
        }
    }
}
