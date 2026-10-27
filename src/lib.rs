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

//! Link — the wire of the Grid: the protocol library that Master Control and every TronGrid
//! Lite instance load as the same shared binary, so the two ends of the wire cannot disagree
//! about what travels over it.
//!
//! Link's design authority — messages, framing, handshake, trust — is `docs/TOPOLOGY.md` in the
//! `tron-grid-lite` repository. Design changes land there before code lands here. The contract
//! of record is the C header, `include/lnk/lnk_protocol.h`; [`protocol`] mirrors it.

pub mod abi;
pub mod codec;
pub mod protocol;
pub mod recording;
pub mod transport;
