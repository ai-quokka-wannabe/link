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
pub const PROTOCOL_VERSION: u32 = 6;

/// `LNK_DEFAULT_PORT`: where Master Control listens when nobody names another port. A default
/// and only a default. The number is the owner's: 30702, from JA-307020 — Tron's program
/// designation — because the port is the doorway into the Grid and Tron is the security
/// program who guards the system.
pub const DEFAULT_PORT: u16 = 30_702;

/// `LNK_KEEPALIVE_PING_MILLIS`: heard nothing for this long — send a PING. The library carries
/// the constant and the obligation; the caller owns the clock.
pub const KEEPALIVE_PING_MILLIS: u32 = 1_000;

/// `LNK_KEEPALIVE_DEAD_MILLIS`: heard nothing for this long — the peer is dead, close the
/// connection. What makes the dead-host liveness rule fire deterministically.
pub const KEEPALIVE_DEAD_MILLIS: u32 = 10_000;

/// `LNK_ACTIONS_REPEAT_TICKS`: how long a connected host's last accepted intent is re-applied
/// when its ACTIONS are merely missing, before zeroed coast. One — repeat the last input, never
/// stall, never rewind — bounded so a longer stall becomes honest coasting.
pub const ACTIONS_REPEAT_TICKS: u32 = 1;

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
    Proprioception = 11,
}

/// What a client is, stated in HELLO. Zero is invalid. A spectator never sends ACTIONS, and the
/// refusal lives in this library on both ends: the sending half refuses to stage ACTIONS on a
/// spectator connection, and the server half treats an ACTIONS frame arriving on one as a
/// protocol violation and closes it.
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
    /// A body sliding along a face: footsteps, a scrape along a wall, a brush past another.
    Scratch = 2,
}

/// `LnkWorldDefinition`: the shared simulation truth, gathered so it can be fingerprinted -
/// the floor physics collides against, the tick length, the standing height. Perception
/// (materials, sensor layouts) is deliberately absent: skew there mis-shades a picture; skew
/// here corrupts the world, silently, which is why the handshake refuses it instead.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorldDefinition {
    pub floor_cells: u32,
    pub floor_cell_size: f32,
    pub floor_height: f32,
    pub relief_amplitude: f32,
    pub relief_wavelength: f32,
    pub relief_octaves: u32,
    pub relief_terraces: u32,
    pub relief_seed: u32,
    pub dt_seconds: f32,
    pub body_half_height: f32,
}

/// The one implementation of the world fingerprint: FNV-1a over the definition's bytes in
/// field order. Exposed through the vtable so every citizen computes its own through this very
/// function - two ends disagreeing can only mean their *values* disagree.
#[must_use]
pub fn world_fingerprint(definition: &WorldDefinition) -> u64 {
    let mut hash: u64 = 14_695_981_039_346_656_037;
    let mut mix = |bits: u32| {
        for byte in 0..4 {
            hash ^= u64::from((bits >> (byte * 8)) & 0xFF);
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
    };
    mix(definition.floor_cells);
    mix(definition.floor_cell_size.to_bits());
    mix(definition.floor_height.to_bits());
    mix(definition.relief_amplitude.to_bits());
    mix(definition.relief_wavelength.to_bits());
    mix(definition.relief_octaves);
    mix(definition.relief_terraces);
    mix(definition.relief_seed);
    mix(definition.dt_seconds.to_bits());
    mix(definition.body_half_height.to_bits());
    hash
}

/// HELLO, client to server, the first frame after the magic. The fingerprint is the raw SHA-256
/// of the C header as the fingerprint tool hashes it; a mismatch earns a human-readable refusal
/// naming both versions, then a closed connection — refusal, not negotiation. The world
/// fingerprint is compared the same way: a client living on a different floor is refused in
/// words, never welcomed into a world it would silently disagree with.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hello {
    pub protocol_version: u32,
    pub fingerprint: [u8; 32],
    /// `Role` as a raw byte; the codec refuses anything but 1 or 2.
    pub role: u8,
    /// Always zero.
    pub reserved0: [u8; 3],
    /// [`world_fingerprint`] over the client's [`WorldDefinition`].
    pub world_fingerprint: u64,
}

/// WELCOME, server to client, the acceptance of a HELLO. After it the server sends the REZ of
/// every live creature and then the next TICK_STATE — late join is not a special case. The
/// world fingerprint travels back too: the skew check bites in both directions.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Welcome {
    /// The tick the next TICK_STATE will carry or exceed.
    pub current_tick: u64,
    /// Seconds per tick, the same number `TglLibraryInfo` carries.
    pub nominal_dt_seconds: f32,
    /// The server's name for this connection.
    pub client_id: u32,
    /// [`world_fingerprint`] over the server's [`WorldDefinition`].
    pub world_fingerprint: u64,
}

/// REZ, both directions: a creature enters the world. The header; then `vertex_count`
/// [`RezVertex`] rows, `triangle_count` [`RezTriangle`] rows, `material_count` [`RezMaterial`]
/// rows, the frame length equal to that sum exactly. What travels of the descriptor is the
/// slice the world needs — the bounds and the contact budget; sensor layouts stay host-local.
/// Bodiless is legitimate: three zero counts, no rows.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rez {
    pub creature_id: u32,
    pub max_forward_speed: f32,
    pub max_turn_rate: f32,
    pub max_vocalisation_strength: f32,
    pub max_contact_count: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub material_count: u32,
}

/// One vertex position in body frame, metres.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RezVertex {
    pub position: [f32; 3],
}

/// One triangle: three vertex indices and the material its surface wears.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RezTriangle {
    pub vertices: [u32; 3],
    pub material: u32,
}

/// One material, exactly the ABI's `TglRenderMaterial` shape: the smooth-limit model.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RezMaterial {
    pub colour: [f32; 3],
    pub index_of_refraction: f32,
    pub emission: [f32; 3],
    pub transmission: f32,
}

/// The three caps of the one variable-size client input. The material cap is the one most
/// likely to be forgotten — it guards the slot space every triangle indexes into.
pub const REZ_MAX_VERTICES: u32 = 1_024;
pub const REZ_MAX_TRIANGLES: u32 = 2_048;
pub const REZ_MAX_MATERIALS: u32 = 16;

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

/// ACTIONS, creature host to server: the Program's staged intent for a future tick — the twelve
/// bytes of `TglActions` plus the address, and the previous tick's twelve piggybacked beside
/// them so one lost or late message loses nothing. The server accepts through the window
/// `[N, N+1)`, latest per (creature, tick) wins, dedupe makes the piggyback free; silence rules
/// and [`ACTIONS_REPEAT_TICKS`] are documented in full on the C header and in TOPOLOGY.md.
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
    /// The tick-1 intent, resent whole. Zeroes when none exists.
    pub previous_forward_speed: f32,
    /// See `previous_forward_speed`.
    pub previous_turn_rate: f32,
    /// See `previous_forward_speed`.
    pub previous_vocalisation: f32,
    /// Always zero. Counted rather than left as invisible alignment padding.
    pub reserved0: [u8; 4],
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

/// `LNK_CONTACTS_MAX`: the most contacts a PROPRIOCEPTION carries, and the most a body may
/// declare - the letter must be able to carry every contact the body feels.
pub const CONTACTS_MAX: u32 = 16;

/// One contact a body felt this tick: where, and the impulse delivered there. Twenty-four bytes.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Contact {
    pub position: [f32; 3],
    pub impulse: [f32; 3],
    /// Unit, world frame: which way the face pushes.
    pub normal: [f32; 3],
    /// Metres past the face before the body was stood back; zero at rest.
    pub depth: f32,
    /// Metres per second along the face, body frame.
    pub slip: [f32; 3],
}

/// PROPRIOCEPTION, server to the one host that owns the creature - a letter, not a broadcast.
/// Every tick after that tick's TICK_STATE: the specific force, whether the feet are on the
/// ground, and `contact_count` [`Contact`] rows in the same frame. Thirty-two bytes.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Proprioception {
    pub tick: u64,
    pub creature_id: u32,
    /// 1 when the feet touch the ground this tick, else 0.
    pub grounded: u8,
    /// Always zero.
    pub reserved0: [u8; 3],
    pub specific_force: [f32; 3],
    pub contact_count: u32,
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
        MessageType::Proprioception => None,
    }
}

// The same numbers the C header pins, pinned again. A sum of the declared fields beside the
// whole struct's size, so a silently grown padding byte fails the build here exactly as it
// fails it there.
const _: () = assert!(size_of::<Hello>() == 4 + 32 + 1 + 3 + 8 && size_of::<Hello>() == 48);
const _: () = assert!(size_of::<Welcome>() == 8 + 4 + 4 + 8 && size_of::<Welcome>() == 24);
const _: () = assert!(size_of::<WorldDefinition>() == 10 * 4 && size_of::<WorldDefinition>() == 40);
const _: () = assert!(size_of::<Rez>() == 8 * 4 && size_of::<Rez>() == 32);
const _: () = assert!(size_of::<RezVertex>() == 12 && size_of::<RezTriangle>() == 16 && size_of::<RezMaterial>() == 32);
const _: () = assert!(
    size_of::<Rez>()
        + REZ_MAX_VERTICES as usize * size_of::<RezVertex>()
        + REZ_MAX_TRIANGLES as usize * size_of::<RezTriangle>()
        + REZ_MAX_MATERIALS as usize * size_of::<RezMaterial>()
        <= FRAME_PAYLOAD_LIMIT
);
const _: () = assert!(size_of::<CreatureState>() == 4 + 12 + 4 + 12 + 4 + 4 && size_of::<CreatureState>() == 40);
const _: () = assert!(size_of::<TickStateHeader>() == 8 + 4 + 4 && size_of::<TickStateHeader>() == 16);
const _: () = assert!(size_of::<Actions>() == 8 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + 4 && size_of::<Actions>() == 40);
const _: () = assert!(size_of::<Event>() == 8 + 12 + 4 + 4 + 1 + 3 && size_of::<Event>() == 32);
const _: () = assert!(size_of::<Derez>() == 8 + 4 + 4 && size_of::<Derez>() == 16);
const _: () = assert!(size_of::<Contact>() == 52);
const _: () = assert!(size_of::<Proprioception>() == 8 + 4 + 1 + 3 + 12 + 4 && size_of::<Proprioception>() == 32);
const _: () = assert!(size_of::<Proprioception>() + CONTACTS_MAX as usize * size_of::<Contact>() <= FRAME_PAYLOAD_LIMIT);
const _: () = assert!(size_of::<Ping>() == 8 && size_of::<Pong>() == 8);
const _: () = assert!(size_of::<TickStateHeader>() + TICK_STATE_MAX_CREATURES as usize * size_of::<CreatureState>() <= FRAME_PAYLOAD_LIMIT);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixed_message_declares_its_exact_size() {
        assert_eq!(exact_payload_bytes(MessageType::Hello), Some(48));
        assert_eq!(exact_payload_bytes(MessageType::Welcome), Some(24));
        assert_eq!(exact_payload_bytes(MessageType::Rez), None);
        assert_eq!(exact_payload_bytes(MessageType::TickState), None);
        assert_eq!(exact_payload_bytes(MessageType::Actions), Some(40));
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
        assert_eq!(PROTOCOL_VERSION, 6);
        assert_eq!(DEFAULT_PORT, 30_702);
    }

    /// The protocol header's own twin check: the two constants that must never drift between
    /// the C side and this mirror, parsed out of the header this crate compiles in.
    #[test]
    fn the_protocol_header_and_this_mirror_agree_on_version_and_port() {
        let header = include_str!("../include/lnk/lnk_protocol.h");
        assert!(
            header.contains(&format!("#define LNK_PROTOCOL_VERSION {PROTOCOL_VERSION}u")),
            "LNK_PROTOCOL_VERSION drifted"
        );
        assert!(header.contains(&format!("#define LNK_DEFAULT_PORT {DEFAULT_PORT}u")), "LNK_DEFAULT_PORT drifted");
        assert!(
            header.contains(&format!("#define LNK_KEEPALIVE_PING_MILLIS {KEEPALIVE_PING_MILLIS}u")),
            "LNK_KEEPALIVE_PING_MILLIS drifted"
        );
        assert!(
            header.contains(&format!("#define LNK_KEEPALIVE_DEAD_MILLIS {KEEPALIVE_DEAD_MILLIS}u")),
            "LNK_KEEPALIVE_DEAD_MILLIS drifted"
        );
        assert!(
            header.contains(&format!("#define LNK_ACTIONS_REPEAT_TICKS {ACTIONS_REPEAT_TICKS}u")),
            "LNK_ACTIONS_REPEAT_TICKS drifted"
        );
        assert!(
            header.contains(&format!("#define LNK_REZ_MAX_VERTICES {REZ_MAX_VERTICES}u")),
            "LNK_REZ_MAX_VERTICES drifted"
        );
        assert!(
            header.contains(&format!("#define LNK_REZ_MAX_TRIANGLES {REZ_MAX_TRIANGLES}u")),
            "LNK_REZ_MAX_TRIANGLES drifted"
        );
        assert!(
            header.contains(&format!("#define LNK_REZ_MAX_MATERIALS {REZ_MAX_MATERIALS}u")),
            "LNK_REZ_MAX_MATERIALS drifted"
        );
    }

    #[test]
    fn the_world_fingerprint_answers_and_discriminates() {
        let definition = WorldDefinition {
            floor_cells: 64,
            floor_cell_size: 2.0,
            floor_height: 0.0,
            relief_amplitude: 5.0,
            relief_wavelength: 46.0,
            relief_octaves: 3,
            relief_terraces: 6,
            relief_seed: 42,
            dt_seconds: 0.031_25,
            body_half_height: 0.05,
        };
        let fingerprint = world_fingerprint(&definition);
        assert_ne!(fingerprint, 0, "a fingerprint of zero would look like an unset field");
        assert_eq!(fingerprint, world_fingerprint(&definition), "the same world answers the same bits");

        let mut other_floor = definition;
        other_floor.relief_seed = 43;
        assert_ne!(world_fingerprint(&other_floor), fingerprint, "a different landscape is a different world");

        let mut other_dt = definition;
        other_dt.dt_seconds = 0.02;
        assert_ne!(world_fingerprint(&other_dt), fingerprint, "a bent dt is a different world too");
    }

    #[test]
    fn actions_carries_the_abis_twelve_bytes_twice_plus_the_address() {
        let intent_bytes = size_of::<f32>() * 3;
        assert_eq!(intent_bytes, 12, "TglActions is three floats; if that moved, this mirror moves with a version bump");
        assert_eq!(
            size_of::<Actions>(),
            8 + 4 + intent_bytes + intent_bytes + 4,
            "current intent plus the previous tick's resent whole, and a counted reserved word instead of invisible padding"
        );
    }
}
