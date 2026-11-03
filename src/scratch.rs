/*
    Copyright (C) 2026 The AiQuokkaWannabe Authors <https://github.com/ai-quokka-wannabe/link>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! Where the tests put the files they write: one folder under the crate's own `target/`.
//!
//! The folder is fixed when the crate is compiled (`CARGO_MANIFEST_DIR`), never read from the
//! environment at run time the way `std::env::temp_dir()` reads `TMP`. Nothing outside the test
//! binary can steer where a test writes, and code scanning, which rightly counts environment
//! variables as user input, has no path to object to.

use std::path::PathBuf;

/// A path for a test's file in `target/test-scratch/`, unique to this process, so parallel test
/// runs never share a file. The folder is created if it is missing.
pub(crate) fn scratch(file_name: &str) -> PathBuf {
    let mut path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/target/test-scratch"));
    std::fs::create_dir_all(&path).expect("the scratch folder");
    path.push(format!("{}-{file_name}", std::process::id()));
    path
}
