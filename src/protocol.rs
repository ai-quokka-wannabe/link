//! The Link protocol's contract, Rust side.
//!
//! The contract of record is the C header, `include/lnk/lnk_protocol.h` — fingerprinted by
//! `tools/check_protocol_version.py`, carried in every HELLO, and read by consumers that never
//! see this crate's source. The types here mirror it field for field, and the asserts at the
//! bottom pin the same sizes the header pins, so the two languages cannot drift apart without
//! one of them refusing to build.
//!
//! Wire structs keep primitive fields rather than Rust enums, deliberately: a `#[repr(u8)]` enum
//! field would make a hostile byte an invalid bit pattern rather than a value to refuse, and the
//! wire's whole doctrine is that hostile bytes are refused, never trusted. The enums exist for
//! the library's own vocabulary; the codec maps bytes to them by refusal.

// The layouts are little-endian on the wire, exactly as an x86-64 host lays them out. A
// big-endian host would need a swap layer that nothing needs today, so it is refused rather
// than half-supported.
#[cfg(target_endian = "big")]
compile_error!("The Link protocol is little-endian on the wire; a big-endian host needs a swap layer that does not exist.");

/// `LNK_PROTOCOL_VERSION`: bumped whenever any declaration changes meaning or layout. The
/// handshake carries the header's fingerprint rather than this number; the number exists for
/// the human-readable refusal.
pub const PROTOCOL_VERSION: u32 = 1;

/// The first four bytes a client ever sends: `LNK1`. Anything else earns a refusal and a closed
/// connection before any frame is read.
pub const MAGIC: [u8; 4] = *b"LNK1";

/// Bytes of frame header on the wire: two of little-endian payload length, one of message type.
/// There is deliberately no struct for it — a struct holding a `u16` and a `u8` is padded to
/// four bytes in both languages, and a layout that exists only with padding suppressed is a
/// trap. Read and write the three bytes as bytes.
pub const FRAME_HEADER_BYTES: usize = 3;

/// The framing's own ceiling: length is a `u16`, so no payload can exceed this.
pub const FRAME_PAYLOAD_LIMIT: usize = 65_535;

/// The most creatures one TICK_STATE may carry. A cap rather than a target — 256 rows is
/// 10,256 bytes against the framing's 65,535 ceiling.
pub const TICK_STATE_MAX_CREATURES: u32 = 256;

/// Message types as they appear in the frame header's third byte. Zero is invalid,
/// deliberately: a zeroed buffer read as a message refuses loudly instead of meaning something.
///
/// `Rez`'s payload layout is not defined yet — it must flatten `TglCreatureDesc`, and that
/// flattening is designed against the flagship's validator as its first consumer. The number is
/// reserved so nothing else ever takes it.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageType {
    Hello = 1,
    Welcome = 2,
    Rez = 3,
    TickState = 4,
    Actions = 5,
    Event = 6,
    Derez = 7,
    Ping = 8,
    Pong = 9,
    Bye = 10,
}

/// What a client is, stated in HELLO. Zero is invalid. A spectator never sends ACTIONS and a
/// server refuses ACTIONS from one.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Spectator = 1,
    CreatureHost = 2,
}

/// Event kinds. Zero is invalid. Events are tick-stamped notifications and never load-bearing
/// state: a client that misses one has missed a sound, not the world.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventKind {
    Vocalisation = 1,
}

/// HELLO, client to server, the first frame after the magic. The fingerprint is the raw SHA-256
/// of the C header as the fingerprint tool hashes it; a mismatch earns a human-readable refusal
/// naming both versions, then a closed connection — refusal, not negotiation.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hello {
    pub protocol_version: u32,
    pub fingerprint: [u8; 32],
    /// `Role` as a raw byte; the codec refuses anything but 1 or 2.
    pub role: u8,
    /// Always zero.
    pub reserved0: [u8; 3],
}

/// WELCOME, server to client, the acceptance of a HELLO. After it the server sends the REZ of
/// every live creature and then the next TICK_STATE — late join is not a special case.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Welcome {
    /// The tick the next TICK_STATE will carry or exceed.
    pub current_tick: u64,
    /// Seconds per tick, the same number `TglLibraryInfo` carries.
    pub nominal_dt_seconds: f32,
    /// The server's name for this connection.
    pub client_id: u32,
}

/// One creature's row in a TICK_STATE: pose, velocity and actuator. Forty bytes, which is what
/// makes a dozen creatures ~500 bytes per tick.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CreatureState {
    /// Stable for the creature's whole rez-to-derez life.
    pub creature_id: u32,
    /// Metres, world space, right-handed, Y up.
    pub position: [f32; 3],
    /// Radians about +Y, right-handed — the roster's own convention.
    pub yaw: f32,
    /// Metres per second, world space.
    pub velocity: [f32; 3],
    /// Radians per second about +Y.
    pub yaw_rate: f32,
    /// The voice actuator as physics settled it, 0 when silent.
    pub vocalisation: f32,
}

/// TICK_STATE, server to every client, every tick: the whole settled world, no deltas, no acks.
/// The payload is this header followed immediately by `creature_count` [`CreatureState`] rows,
/// and the frame length must equal `size_of::<TickStateHeader>() + creature_count * size_of::<CreatureState>()`
/// with `creature_count <= TICK_STATE_MAX_CREATURES` — both checked before any copy.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TickStateHeader {
    /// The tick these rows are the settled truth of.
    pub tick: u64,
    pub creature_count: u32,
    /// Always zero.
    pub reserved0: [u8; 4],
}

/// ACTIONS, creature host to server: the Program's staged intent for a future tick, exactly the
/// twelve bytes of `TglActions` plus the address. The server executes the latest ACTIONS with
/// tick ≤ the tick being stepped and discards stragglers; a creature with no ACTIONS coasts.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Actions {
    /// The tick this intent is staged for.
    pub tick: u64,
    pub creature_id: u32,
    /// Metres per second, clamped server-side to the body.
    pub desired_forward_speed: f32,
    /// Radians per second, clamped server-side to the body.
    pub desired_turn_rate: f32,
    /// 0 to 1, clamped server-side.
    pub vocalisation_strength: f32,
}

/// EVENT, server to every client: a tick-stamped notification, never load-bearing state. The
/// spectator synthesises its audio from these.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub tick: u64,
    /// Where it happened, metres, world space.
    pub position: [f32; 3],
    /// Kind-specific magnitude; for a vocalisation, the actuator value.
    pub strength: f32,
    /// Who caused it.
    pub creature_id: u32,
    /// `EventKind` as a raw byte; the codec refuses anything unknown.
    pub kind: u8,
    /// Always zero.
    pub reserved0: [u8; 3],
}

/// DEREZ, server to every client: the creature leaves the world at this tick.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Derez {
    pub tick: u64,
    pub creature_id: u32,
    /// Always zero.
    pub reserved0: [u8; 4],
}

/// PING, either direction. The nonce comes back verbatim in a PONG, so each end measures its
/// own round trip without trusting the other's clock.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ping {
    pub nonce: u64,
}

/// PONG, the answer to a PING, carrying the same nonce.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pong {
    pub nonce: u64,
}

/// The exact payload size a fixed-size message must have — not a maximum: a frame whose length
/// differs is refused before any copy. `None` for the two messages whose size is not a single
/// number: TICK_STATE (header plus rows, rule on [`TickStateHeader`]) and REZ (layout reserved).
/// BYE is fixed at zero: a courtesy carries nothing.
#[must_use]
pub const fn exact_payload_bytes(message: MessageType) -> Option<usize> {
    match message {
        MessageType::Hello => Some(size_of::<Hello>()),
        MessageType::Welcome => Some(size_of::<Welcome>()),
        MessageType::Rez => None,
        MessageType::TickState => None,
        MessageType::Actions => Some(size_of::<Actions>()),
        MessageType::Event => Some(size_of::<Event>()),
        MessageType::Derez => Some(size_of::<Derez>()),
        MessageType::Ping => Some(size_of::<Ping>()),
        MessageType::Pong => Some(size_of::<Pong>()),
        MessageType::Bye => Some(0),
    }
}

// The same numbers the C header pins, pinned again. A sum of the declared fields beside the
// whole struct's size, so a silently grown padding byte fails the build here exactly as it
// fails it there.
const _: () = assert!(size_of::<Hello>() == 4 + 32 + 1 + 3 && size_of::<Hello>() == 40);
const _: () = assert!(size_of::<Welcome>() == 8 + 4 + 4 && size_of::<Welcome>() == 16);
const _: () = assert!(size_of::<CreatureState>() == 4 + 12 + 4 + 12 + 4 + 4 && size_of::<CreatureState>() == 40);
const _: () = assert!(size_of::<TickStateHeader>() == 8 + 4 + 4 && size_of::<TickStateHeader>() == 16);
const _: () = assert!(size_of::<Actions>() == 8 + 4 + 4 + 4 + 4 && size_of::<Actions>() == 24);
const _: () = assert!(size_of::<Event>() == 8 + 12 + 4 + 4 + 1 + 3 && size_of::<Event>() == 32);
const _: () = assert!(size_of::<Derez>() == 8 + 4 + 4 && size_of::<Derez>() == 16);
const _: () = assert!(size_of::<Ping>() == 8 && size_of::<Pong>() == 8);
const _: () = assert!(size_of::<TickStateHeader>() + TICK_STATE_MAX_CREATURES as usize * size_of::<CreatureState>() <= FRAME_PAYLOAD_LIMIT);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixed_message_declares_its_exact_size() {
        assert_eq!(exact_payload_bytes(MessageType::Hello), Some(40));
        assert_eq!(exact_payload_bytes(MessageType::Welcome), Some(16));
        assert_eq!(exact_payload_bytes(MessageType::Rez), None);
        assert_eq!(exact_payload_bytes(MessageType::TickState), None);
        assert_eq!(exact_payload_bytes(MessageType::Actions), Some(24));
        assert_eq!(exact_payload_bytes(MessageType::Event), Some(32));
        assert_eq!(exact_payload_bytes(MessageType::Derez), Some(16));
        assert_eq!(exact_payload_bytes(MessageType::Ping), Some(8));
        assert_eq!(exact_payload_bytes(MessageType::Pong), Some(8));
        assert_eq!(exact_payload_bytes(MessageType::Bye), Some(0));
    }

    #[test]
    fn every_fixed_payload_fits_one_frame() {
        for message in [
            MessageType::Hello,
            MessageType::Welcome,
            MessageType::Actions,
            MessageType::Event,
            MessageType::Derez,
            MessageType::Ping,
            MessageType::Pong,
            MessageType::Bye,
        ] {
            let bytes = exact_payload_bytes(message).expect("fixed messages declare a size");
            assert!(bytes <= FRAME_PAYLOAD_LIMIT, "{message:?} cannot fit one frame");
        }
    }

    #[test]
    fn a_full_tick_state_fits_one_frame_with_room_to_grow() {
        let full = size_of::<TickStateHeader>() + TICK_STATE_MAX_CREATURES as usize * size_of::<CreatureState>();
        assert_eq!(full, 10_256);
        assert!(
            full * 4 <= FRAME_PAYLOAD_LIMIT,
            "the cap is meant to be able to quadruple before the framing is interesting"
        );
    }

    #[test]
    fn the_magic_is_lnk1_and_the_type_numbers_are_the_headers() {
        assert_eq!(MAGIC, [0x4C, 0x4E, 0x4B, 0x31]);
        assert_eq!(MessageType::Hello as u8, 1);
        assert_eq!(MessageType::Bye as u8, 10);
        assert_eq!(Role::Spectator as u8, 1);
        assert_eq!(Role::CreatureHost as u8, 2);
        assert_eq!(EventKind::Vocalisation as u8, 1);
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn actions_carries_exactly_the_abis_twelve_bytes_plus_the_address() {
        let intent_bytes = size_of::<f32>() * 3;
        assert_eq!(intent_bytes, 12, "TglActions is three floats; if that moved, this mirror moves with a version bump");
        assert_eq!(size_of::<Actions>(), 8 + 4 + intent_bytes);
    }
}
