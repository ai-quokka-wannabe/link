//! Bytes to messages and back, by refusal.
//!
//! The codec is pure: no sockets, no allocation before validation, no `unsafe`. Every field is
//! read and written explicitly in little-endian, so nothing here reinterprets memory and the
//! wire's byte order is a fact of the code rather than of the host. Strictness is symmetric:
//! [`encode`] refuses to produce any frame [`decode`] would refuse to accept, so an invalid
//! frame from this library is unrepresentable rather than unlikely.
//!
//! The receiving order is the audit's gold-plated rule made into an API: read the three header
//! bytes, ask [`payload_rule`] what the type may carry — refusing unknown types and impossible
//! lengths *before* any payload is read or copied — and only then hand the payload to
//! [`decode`]. The codec never panics on any input; every failure is a named [`DecodeError`].

use crate::protocol::{
    Actions, CreatureState, Derez, Event, EventKind, FRAME_HEADER_BYTES, Hello, MessageType, Ping, Pong, REZ_MAX_MATERIALS, REZ_MAX_TRIANGLES, REZ_MAX_VERTICES, Rez,
    RezMaterial, RezTriangle, RezVertex, Role, TICK_STATE_MAX_CREATURES, TickStateHeader, Welcome,
};

/// A decoded message, owning its payload. TICK_STATE's rows live in a `Vec` sized only after
/// the count has been validated against the cap and the length — bounded allocation, after
/// refusal has had its chance.
#[derive(Clone, PartialEq, Debug)]
pub enum Message {
    Hello(Hello),
    Welcome(Welcome),
    Rez {
        header: Rez,
        vertices: Vec<RezVertex>,
        triangles: Vec<RezTriangle>,
        materials: Vec<RezMaterial>,
    },
    TickState {
        header: TickStateHeader,
        states: Vec<CreatureState>,
    },
    Actions(Actions),
    Event(Event),
    Derez(Derez),
    Ping(Ping),
    Pong(Pong),
    Bye,
}

/// Why a frame was refused. Every variant names the field that betrayed it, because a refusal
/// that cannot be diagnosed gets worked around instead of fixed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// The type byte is not a v1 message. REZ lands here too: its number is reserved and its
    /// payload undefined, so an end that receives one refuses it as unknown.
    UnknownOrReservedType(u8),
    /// The payload length does not match what the type demands.
    WrongLength { expected: usize, got: usize },
    /// A TICK_STATE length that cannot be a header plus whole rows.
    RaggedTickState { got: usize },
    /// A TICK_STATE whose row count exceeds the cap.
    CountOverCap { count: u32 },
    /// A TICK_STATE whose declared count disagrees with its length.
    CountLengthMismatch { count: u32, rows_by_length: usize },
    /// A role byte that is neither spectator nor creature host.
    InvalidRole(u8),
    /// An event kind byte no v1 end emits.
    InvalidEventKind(u8),
    /// Reserved bytes must be zero. A nonzero one is either corruption or a future version
    /// talking to a past one, and both deserve refusal rather than a shrug.
    ReservedNotZero,
    /// A REZ count beyond its cap — refused before any row is read, because the one
    /// variable-size client input is exactly where parsers die.
    RezCountOverCap { vertices: u32, triangles: u32, materials: u32 },
    /// A REZ whose declared counts do not sum to its frame length.
    RezLengthMismatch { expected: usize, got: usize },
    /// A triangle naming a vertex or material that does not exist.
    RezIndexOutOfRange { triangle: u32 },
    /// A REZ float that is not a real number. A NaN vertex entering a hierarchy poisons a
    /// traversal that fails somewhere else entirely, so it never crosses the wire.
    RezNotFinite,
}

/// Why a message was refused at the sending end. Encode refuses exactly what decode refuses,
/// minus the errors only raw bytes can have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncodeError {
    CountOverCap {
        count: usize,
    },
    /// A TICK_STATE header whose declared count disagrees with the rows actually supplied.
    CountRowsMismatch {
        count: u32,
        rows: usize,
    },
    InvalidRole(u8),
    InvalidEventKind(u8),
    ReservedNotZero,
    /// The REZ refusals, sending side — encode refuses exactly what decode refuses.
    RezCountOverCap {
        vertices: u32,
        triangles: u32,
        materials: u32,
    },
    RezCountRowsMismatch,
    RezIndexOutOfRange {
        triangle: u32,
    },
    RezNotFinite,
}

/// What a type byte's payload may look like, answerable from the three header bytes alone —
/// before any payload is read, copied or allocated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PayloadRule {
    /// The length must equal this exactly.
    Exact(usize),
    /// TICK_STATE: the length must be `header + rows * row` with `rows <= TICK_STATE_MAX_CREATURES`.
    TickState,
    /// REZ: at least its header, at most a maximal body; the counts inside the header then
    /// judge the exact sum before any row is copied.
    Rez,
}

const TICK_HEADER_BYTES: usize = size_of::<TickStateHeader>();
const CREATURE_STATE_BYTES: usize = size_of::<CreatureState>();
const REZ_HEADER_BYTES: usize = size_of::<Rez>();

const fn max_rez_payload_bytes() -> usize {
    REZ_HEADER_BYTES
        + REZ_MAX_VERTICES as usize * size_of::<RezVertex>()
        + REZ_MAX_TRIANGLES as usize * size_of::<RezTriangle>()
        + REZ_MAX_MATERIALS as usize * size_of::<RezMaterial>()
}

/// The most bytes any legal frame occupies on the wire, header included — a receive buffer this
/// big can hold anything a lawful end may send. A maximal REZ outweighs a full TICK_STATE.
#[must_use]
pub const fn max_frame_bytes() -> usize {
    let tick = TICK_HEADER_BYTES + TICK_STATE_MAX_CREATURES as usize * CREATURE_STATE_BYTES;
    let rez = max_rez_payload_bytes();
    FRAME_HEADER_BYTES + if rez > tick { rez } else { tick }
}

/// The three header bytes, decoded: payload length and raw type byte. The inverse of what
/// [`encode`] writes first.
#[must_use]
pub const fn decode_frame_header(header: [u8; 3]) -> (u16, u8) {
    (u16::from_le_bytes([header[0], header[1]]), header[2])
}

/// The rule a type byte's payload must satisfy, or the refusal for a type no v1 end speaks.
/// This is the before-any-copy gate: call it with the header bytes' type, check the length
/// against the answer, and only then read the payload.
pub const fn payload_rule(type_byte: u8) -> Result<PayloadRule, DecodeError> {
    match type_byte {
        t if t == MessageType::Hello as u8 => Ok(PayloadRule::Exact(size_of::<Hello>())),
        t if t == MessageType::Welcome as u8 => Ok(PayloadRule::Exact(size_of::<Welcome>())),
        t if t == MessageType::Rez as u8 => Ok(PayloadRule::Rez),
        t if t == MessageType::TickState as u8 => Ok(PayloadRule::TickState),
        t if t == MessageType::Actions as u8 => Ok(PayloadRule::Exact(size_of::<Actions>())),
        t if t == MessageType::Event as u8 => Ok(PayloadRule::Exact(size_of::<Event>())),
        t if t == MessageType::Derez as u8 => Ok(PayloadRule::Exact(size_of::<Derez>())),
        t if t == MessageType::Ping as u8 => Ok(PayloadRule::Exact(size_of::<Ping>())),
        t if t == MessageType::Pong as u8 => Ok(PayloadRule::Exact(size_of::<Pong>())),
        t if t == MessageType::Bye as u8 => Ok(PayloadRule::Exact(0)),
        other => Err(DecodeError::UnknownOrReservedType(other)),
    }
}

/// The length check the rule implies, separated from [`payload_rule`] so the transport can run
/// it on the header alone and hang up without reading a byte of a hostile payload.
pub const fn check_length(rule: PayloadRule, length: usize) -> Result<(), DecodeError> {
    match rule {
        PayloadRule::Exact(expected) => {
            if length == expected {
                Ok(())
            } else {
                Err(DecodeError::WrongLength { expected, got: length })
            }
        }
        PayloadRule::TickState => {
            if length < TICK_HEADER_BYTES || !(length - TICK_HEADER_BYTES).is_multiple_of(CREATURE_STATE_BYTES) {
                return Err(DecodeError::RaggedTickState { got: length });
            }
            let rows = (length - TICK_HEADER_BYTES) / CREATURE_STATE_BYTES;
            if rows > TICK_STATE_MAX_CREATURES as usize {
                return Err(DecodeError::CountOverCap { count: rows as u32 });
            }
            Ok(())
        }
        PayloadRule::Rez => {
            // The header-time bound: the counts live inside the payload, so the exact sum is
            // judged in decode — but a frame that cannot possibly be a REZ dies here, before
            // the transport asks the socket for its payload.
            if length < REZ_HEADER_BYTES || length > max_rez_payload_bytes() {
                Err(DecodeError::RezLengthMismatch {
                    expected: REZ_HEADER_BYTES,
                    got: length,
                })
            } else {
                Ok(())
            }
        }
    }
}

/// A whole payload to a message, every refusal a named error, no panic on any input. The
/// payload must already have passed [`check_length`] for its rule; this function re-checks
/// rather than trusts, because "the caller surely validated" is how parsers die.
pub fn decode(type_byte: u8, payload: &[u8]) -> Result<Message, DecodeError> {
    let rule = payload_rule(type_byte)?;
    check_length(rule, payload.len())?;

    match rule {
        PayloadRule::TickState => decode_tick_state(payload),
        PayloadRule::Rez => decode_rez(payload),
        PayloadRule::Exact(_) => match type_byte {
            t if t == MessageType::Hello as u8 => decode_hello(payload),
            t if t == MessageType::Welcome as u8 => Ok(Message::Welcome(Welcome {
                current_tick: read_u64(payload, 0),
                nominal_dt_seconds: read_f32(payload, 8),
                client_id: read_u32(payload, 12),
                world_fingerprint: read_u64(payload, 16),
            })),
            t if t == MessageType::Actions as u8 => {
                if payload[36..40] != [0; 4] {
                    return Err(DecodeError::ReservedNotZero);
                }
                Ok(Message::Actions(Actions {
                    tick: read_u64(payload, 0),
                    creature_id: read_u32(payload, 8),
                    desired_forward_speed: read_f32(payload, 12),
                    desired_turn_rate: read_f32(payload, 16),
                    vocalisation_strength: read_f32(payload, 20),
                    previous_forward_speed: read_f32(payload, 24),
                    previous_turn_rate: read_f32(payload, 28),
                    previous_vocalisation: read_f32(payload, 32),
                    reserved0: [0; 4],
                }))
            }
            t if t == MessageType::Event as u8 => decode_event(payload),
            t if t == MessageType::Derez as u8 => {
                if payload[12..16] != [0u8; 4] {
                    return Err(DecodeError::ReservedNotZero);
                }
                Ok(Message::Derez(Derez {
                    tick: read_u64(payload, 0),
                    creature_id: read_u32(payload, 8),
                    reserved0: [0; 4],
                }))
            }
            t if t == MessageType::Ping as u8 => Ok(Message::Ping(Ping { nonce: read_u64(payload, 0) })),
            t if t == MessageType::Pong as u8 => Ok(Message::Pong(Pong { nonce: read_u64(payload, 0) })),
            t if t == MessageType::Bye as u8 => Ok(Message::Bye),
            other => Err(DecodeError::UnknownOrReservedType(other)),
        },
    }
}

fn decode_hello(payload: &[u8]) -> Result<Message, DecodeError> {
    let role = payload[36];
    if role != Role::Spectator as u8 && role != Role::CreatureHost as u8 {
        return Err(DecodeError::InvalidRole(role));
    }
    if payload[37..40] != [0u8; 3] {
        return Err(DecodeError::ReservedNotZero);
    }
    let mut fingerprint = [0u8; 32];
    fingerprint.copy_from_slice(&payload[4..36]);
    Ok(Message::Hello(Hello {
        protocol_version: read_u32(payload, 0),
        fingerprint,
        role,
        reserved0: [0; 3],
        world_fingerprint: read_u64(payload, 40),
    }))
}

/// The one variable-size client input, judged whole before a single row is copied: counts
/// against caps, the exact length against the counts, every index against its count, every
/// float for finiteness. Single-pass, refuses entire — the shape that survives its postmortems.
fn decode_rez(payload: &[u8]) -> Result<Message, DecodeError> {
    let header = Rez {
        creature_id: read_u32(payload, 0),
        max_forward_speed: read_f32(payload, 4),
        max_turn_rate: read_f32(payload, 8),
        max_vocalisation_strength: read_f32(payload, 12),
        max_contact_count: read_u32(payload, 16),
        vertex_count: read_u32(payload, 20),
        triangle_count: read_u32(payload, 24),
        material_count: read_u32(payload, 28),
    };

    if header.vertex_count > REZ_MAX_VERTICES || header.triangle_count > REZ_MAX_TRIANGLES || header.material_count > REZ_MAX_MATERIALS {
        return Err(DecodeError::RezCountOverCap {
            vertices: header.vertex_count,
            triangles: header.triangle_count,
            materials: header.material_count,
        });
    }

    let expected = REZ_HEADER_BYTES
        + header.vertex_count as usize * size_of::<RezVertex>()
        + header.triangle_count as usize * size_of::<RezTriangle>()
        + header.material_count as usize * size_of::<RezMaterial>();
    if payload.len() != expected {
        return Err(DecodeError::RezLengthMismatch { expected, got: payload.len() });
    }

    if !header.max_forward_speed.is_finite() || !header.max_turn_rate.is_finite() || !header.max_vocalisation_strength.is_finite() {
        return Err(DecodeError::RezNotFinite);
    }

    let mut at = REZ_HEADER_BYTES;
    let mut vertices = Vec::with_capacity(header.vertex_count as usize);
    for _ in 0..header.vertex_count {
        let position = [read_f32(payload, at), read_f32(payload, at + 4), read_f32(payload, at + 8)];
        if position.iter().any(|axis| !axis.is_finite()) {
            return Err(DecodeError::RezNotFinite);
        }
        vertices.push(RezVertex { position });
        at += size_of::<RezVertex>();
    }

    let mut triangles = Vec::with_capacity(header.triangle_count as usize);
    for index in 0..header.triangle_count {
        let corner = [read_u32(payload, at), read_u32(payload, at + 4), read_u32(payload, at + 8)];
        let material = read_u32(payload, at + 12);
        if corner.iter().any(|vertex| *vertex >= header.vertex_count) || material >= header.material_count {
            return Err(DecodeError::RezIndexOutOfRange { triangle: index });
        }
        triangles.push(RezTriangle { vertices: corner, material });
        at += size_of::<RezTriangle>();
    }

    let mut materials = Vec::with_capacity(header.material_count as usize);
    for _ in 0..header.material_count {
        let material = RezMaterial {
            colour: [read_f32(payload, at), read_f32(payload, at + 4), read_f32(payload, at + 8)],
            index_of_refraction: read_f32(payload, at + 12),
            emission: [read_f32(payload, at + 16), read_f32(payload, at + 20), read_f32(payload, at + 24)],
            transmission: read_f32(payload, at + 28),
        };
        let fields = [
            material.colour[0],
            material.colour[1],
            material.colour[2],
            material.index_of_refraction,
            material.emission[0],
            material.emission[1],
            material.emission[2],
            material.transmission,
        ];
        if fields.iter().any(|field| !field.is_finite()) {
            return Err(DecodeError::RezNotFinite);
        }
        materials.push(material);
        at += size_of::<RezMaterial>();
    }

    Ok(Message::Rez {
        header,
        vertices,
        triangles,
        materials,
    })
}

fn decode_event(payload: &[u8]) -> Result<Message, DecodeError> {
    let kind = payload[28];
    if kind != EventKind::Vocalisation as u8 {
        return Err(DecodeError::InvalidEventKind(kind));
    }
    if payload[29..32] != [0u8; 3] {
        return Err(DecodeError::ReservedNotZero);
    }
    Ok(Message::Event(Event {
        tick: read_u64(payload, 0),
        position: [read_f32(payload, 8), read_f32(payload, 12), read_f32(payload, 16)],
        strength: read_f32(payload, 20),
        creature_id: read_u32(payload, 24),
        kind,
        reserved0: [0; 3],
    }))
}

fn decode_tick_state(payload: &[u8]) -> Result<Message, DecodeError> {
    let count = read_u32(payload, 8);
    if payload[12..16] != [0u8; 4] {
        return Err(DecodeError::ReservedNotZero);
    }
    let rows_by_length = (payload.len() - TICK_HEADER_BYTES) / CREATURE_STATE_BYTES;
    if count as usize != rows_by_length {
        return Err(DecodeError::CountLengthMismatch { count, rows_by_length });
    }

    let mut states = Vec::with_capacity(rows_by_length);
    for row in 0..rows_by_length {
        let at = TICK_HEADER_BYTES + row * CREATURE_STATE_BYTES;
        states.push(CreatureState {
            creature_id: read_u32(payload, at),
            position: [read_f32(payload, at + 4), read_f32(payload, at + 8), read_f32(payload, at + 12)],
            yaw: read_f32(payload, at + 16),
            velocity: [read_f32(payload, at + 20), read_f32(payload, at + 24), read_f32(payload, at + 28)],
            yaw_rate: read_f32(payload, at + 32),
            vocalisation: read_f32(payload, at + 36),
        });
    }
    Ok(Message::TickState {
        header: TickStateHeader {
            tick: read_u64(payload, 0),
            creature_count: count,
            reserved0: [0; 4],
        },
        states,
    })
}

/// A message to a whole frame — header and payload — appended to `out`. Refuses exactly what
/// [`decode`] refuses: an invalid frame from this end is unrepresentable, not unlikely.
pub fn encode(message: &Message, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    match message {
        Message::Hello(hello) => {
            if hello.role != Role::Spectator as u8 && hello.role != Role::CreatureHost as u8 {
                return Err(EncodeError::InvalidRole(hello.role));
            }
            if hello.reserved0 != [0; 3] {
                return Err(EncodeError::ReservedNotZero);
            }
            frame_header(out, MessageType::Hello, size_of::<Hello>());
            out.extend_from_slice(&hello.protocol_version.to_le_bytes());
            out.extend_from_slice(&hello.fingerprint);
            out.push(hello.role);
            out.extend_from_slice(&hello.reserved0);
            out.extend_from_slice(&hello.world_fingerprint.to_le_bytes());
        }
        Message::Welcome(welcome) => {
            frame_header(out, MessageType::Welcome, size_of::<Welcome>());
            out.extend_from_slice(&welcome.current_tick.to_le_bytes());
            out.extend_from_slice(&welcome.nominal_dt_seconds.to_le_bytes());
            out.extend_from_slice(&welcome.client_id.to_le_bytes());
            out.extend_from_slice(&welcome.world_fingerprint.to_le_bytes());
        }
        Message::Rez {
            header,
            vertices,
            triangles,
            materials,
        } => {
            if header.vertex_count > REZ_MAX_VERTICES || header.triangle_count > REZ_MAX_TRIANGLES || header.material_count > REZ_MAX_MATERIALS {
                return Err(EncodeError::RezCountOverCap {
                    vertices: header.vertex_count,
                    triangles: header.triangle_count,
                    materials: header.material_count,
                });
            }
            if header.vertex_count as usize != vertices.len() || header.triangle_count as usize != triangles.len() || header.material_count as usize != materials.len() {
                return Err(EncodeError::RezCountRowsMismatch);
            }
            if !header.max_forward_speed.is_finite() || !header.max_turn_rate.is_finite() || !header.max_vocalisation_strength.is_finite() {
                return Err(EncodeError::RezNotFinite);
            }
            for (index, triangle) in triangles.iter().enumerate() {
                if triangle.vertices.iter().any(|vertex| *vertex >= header.vertex_count) || triangle.material >= header.material_count {
                    #[allow(clippy::cast_possible_truncation)]
                    return Err(EncodeError::RezIndexOutOfRange { triangle: index as u32 });
                }
            }
            for vertex in vertices {
                if vertex.position.iter().any(|axis| !axis.is_finite()) {
                    return Err(EncodeError::RezNotFinite);
                }
            }
            for material in materials {
                let fields = [
                    material.colour[0],
                    material.colour[1],
                    material.colour[2],
                    material.index_of_refraction,
                    material.emission[0],
                    material.emission[1],
                    material.emission[2],
                    material.transmission,
                ];
                if fields.iter().any(|field| !field.is_finite()) {
                    return Err(EncodeError::RezNotFinite);
                }
            }

            let bytes =
                REZ_HEADER_BYTES + vertices.len() * size_of::<RezVertex>() + triangles.len() * size_of::<RezTriangle>() + materials.len() * size_of::<RezMaterial>();
            frame_header(out, MessageType::Rez, bytes);
            out.extend_from_slice(&header.creature_id.to_le_bytes());
            out.extend_from_slice(&header.max_forward_speed.to_le_bytes());
            out.extend_from_slice(&header.max_turn_rate.to_le_bytes());
            out.extend_from_slice(&header.max_vocalisation_strength.to_le_bytes());
            out.extend_from_slice(&header.max_contact_count.to_le_bytes());
            out.extend_from_slice(&header.vertex_count.to_le_bytes());
            out.extend_from_slice(&header.triangle_count.to_le_bytes());
            out.extend_from_slice(&header.material_count.to_le_bytes());
            for vertex in vertices {
                for axis in vertex.position {
                    out.extend_from_slice(&axis.to_le_bytes());
                }
            }
            for triangle in triangles {
                for corner in triangle.vertices {
                    out.extend_from_slice(&corner.to_le_bytes());
                }
                out.extend_from_slice(&triangle.material.to_le_bytes());
            }
            for material in materials {
                for channel in material.colour {
                    out.extend_from_slice(&channel.to_le_bytes());
                }
                out.extend_from_slice(&material.index_of_refraction.to_le_bytes());
                for channel in material.emission {
                    out.extend_from_slice(&channel.to_le_bytes());
                }
                out.extend_from_slice(&material.transmission.to_le_bytes());
            }
        }
        Message::TickState { header, states } => {
            if states.len() > TICK_STATE_MAX_CREATURES as usize {
                return Err(EncodeError::CountOverCap { count: states.len() });
            }
            if header.reserved0 != [0; 4] {
                return Err(EncodeError::ReservedNotZero);
            }
            if header.creature_count as usize != states.len() {
                // A header disagreeing with its own rows is the mismatch decode refuses — named
                // as what it is, not mislabelled as a cap violation.
                return Err(EncodeError::CountRowsMismatch {
                    count: header.creature_count,
                    rows: states.len(),
                });
            }
            frame_header(out, MessageType::TickState, TICK_HEADER_BYTES + states.len() * CREATURE_STATE_BYTES);
            out.extend_from_slice(&header.tick.to_le_bytes());
            out.extend_from_slice(&header.creature_count.to_le_bytes());
            out.extend_from_slice(&header.reserved0);
            for state in states {
                out.extend_from_slice(&state.creature_id.to_le_bytes());
                for axis in state.position {
                    out.extend_from_slice(&axis.to_le_bytes());
                }
                out.extend_from_slice(&state.yaw.to_le_bytes());
                for axis in state.velocity {
                    out.extend_from_slice(&axis.to_le_bytes());
                }
                out.extend_from_slice(&state.yaw_rate.to_le_bytes());
                out.extend_from_slice(&state.vocalisation.to_le_bytes());
            }
        }
        Message::Actions(actions) => {
            if actions.reserved0 != [0; 4] {
                return Err(EncodeError::ReservedNotZero);
            }
            frame_header(out, MessageType::Actions, size_of::<Actions>());
            out.extend_from_slice(&actions.tick.to_le_bytes());
            out.extend_from_slice(&actions.creature_id.to_le_bytes());
            out.extend_from_slice(&actions.desired_forward_speed.to_le_bytes());
            out.extend_from_slice(&actions.desired_turn_rate.to_le_bytes());
            out.extend_from_slice(&actions.vocalisation_strength.to_le_bytes());
            out.extend_from_slice(&actions.previous_forward_speed.to_le_bytes());
            out.extend_from_slice(&actions.previous_turn_rate.to_le_bytes());
            out.extend_from_slice(&actions.previous_vocalisation.to_le_bytes());
            out.extend_from_slice(&actions.reserved0);
        }
        Message::Event(event) => {
            if event.kind != EventKind::Vocalisation as u8 {
                return Err(EncodeError::InvalidEventKind(event.kind));
            }
            if event.reserved0 != [0; 3] {
                return Err(EncodeError::ReservedNotZero);
            }
            frame_header(out, MessageType::Event, size_of::<Event>());
            out.extend_from_slice(&event.tick.to_le_bytes());
            for axis in event.position {
                out.extend_from_slice(&axis.to_le_bytes());
            }
            out.extend_from_slice(&event.strength.to_le_bytes());
            out.extend_from_slice(&event.creature_id.to_le_bytes());
            out.push(event.kind);
            out.extend_from_slice(&event.reserved0);
        }
        Message::Derez(derez) => {
            if derez.reserved0 != [0; 4] {
                return Err(EncodeError::ReservedNotZero);
            }
            frame_header(out, MessageType::Derez, size_of::<Derez>());
            out.extend_from_slice(&derez.tick.to_le_bytes());
            out.extend_from_slice(&derez.creature_id.to_le_bytes());
            out.extend_from_slice(&derez.reserved0);
        }
        Message::Ping(ping) => {
            frame_header(out, MessageType::Ping, size_of::<Ping>());
            out.extend_from_slice(&ping.nonce.to_le_bytes());
        }
        Message::Pong(pong) => {
            frame_header(out, MessageType::Pong, size_of::<Pong>());
            out.extend_from_slice(&pong.nonce.to_le_bytes());
        }
        Message::Bye => frame_header(out, MessageType::Bye, 0),
    }
    Ok(())
}

/// The three header bytes. `length` is always a validated payload size well under the u16
/// ceiling — the protocol asserts pin that — so the cast cannot truncate.
fn frame_header(out: &mut Vec<u8>, message: MessageType, length: usize) {
    let length = length as u16;
    out.extend_from_slice(&length.to_le_bytes());
    out.push(message as u8);
}

// Field readers. Callers have already proved the payload long enough through check_length, and
// each reader re-proves it structurally: a wrong offset is a slice-bounds panic in every test
// run rather than a silent misread. The panic is unreachable from any input that passed the
// length gate, which the never-panics test demonstrates by bombardment.
fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    let mut eight = [0u8; 8];
    eight.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(eight)
}

fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FRAME_PAYLOAD_LIMIT, PROTOCOL_VERSION};

    fn sample_hello() -> Message {
        Message::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            fingerprint: [0xA5; 32],
            role: Role::CreatureHost as u8,
            reserved0: [0; 3],
            world_fingerprint: 0x5EED,
        })
    }

    /// A REZ with every count as asked, each triangle indexing real vertices and a real
    /// material - the body the tests rez, up to the largest one the wire admits.
    fn sample_rez(vertices: u32, triangles: u32, materials: u32) -> Message {
        let header = Rez {
            creature_id: 42,
            max_forward_speed: 1.0,
            max_turn_rate: 1.5,
            max_vocalisation_strength: 1.0,
            max_contact_count: 4,
            vertex_count: vertices,
            triangle_count: triangles,
            material_count: materials,
        };
        Message::Rez {
            header,
            vertices: (0..vertices)
                .map(|index| RezVertex {
                    position: [index as f32, 0.5, -1.0],
                })
                .collect(),
            triangles: (0..triangles)
                .map(|index| RezTriangle {
                    vertices: [index % vertices.max(1), (index + 1) % vertices.max(1), (index + 2) % vertices.max(1)],
                    material: index % materials.max(1),
                })
                .collect(),
            materials: (0..materials)
                .map(|index| RezMaterial {
                    colour: [0.1, 0.2, index as f32],
                    index_of_refraction: 1.5,
                    emission: [0.0; 3],
                    transmission: 0.0,
                })
                .collect(),
        }
    }

    fn sample_tick_state(rows: u32) -> Message {
        let states = (0..rows)
            .map(|index| CreatureState {
                creature_id: index,
                position: [1.0, 2.0, 3.0],
                yaw: 0.5,
                velocity: [-1.0, 0.0, 4.5],
                yaw_rate: -0.25,
                vocalisation: 0.75,
            })
            .collect();
        Message::TickState {
            header: TickStateHeader {
                tick: 777,
                creature_count: rows,
                reserved0: [0; 4],
            },
            states,
        }
    }

    fn everything() -> Vec<Message> {
        vec![
            sample_hello(),
            sample_rez(0, 0, 0),
            sample_rez(4, 2, 1),
            sample_rez(REZ_MAX_VERTICES, REZ_MAX_TRIANGLES, REZ_MAX_MATERIALS),
            Message::Welcome(Welcome {
                current_tick: 41,
                nominal_dt_seconds: 0.03125,
                client_id: 7,
                world_fingerprint: 0x5EED,
            }),
            sample_tick_state(0),
            sample_tick_state(3),
            sample_tick_state(TICK_STATE_MAX_CREATURES),
            Message::Actions(Actions {
                tick: 42,
                creature_id: 3,
                desired_forward_speed: 1.5,
                desired_turn_rate: -0.5,
                vocalisation_strength: 1.0,
                previous_forward_speed: 1.25,
                previous_turn_rate: 0.5,
                previous_vocalisation: 0.0,
                reserved0: [0; 4],
            }),
            Message::Event(Event {
                tick: 43,
                position: [10.0, 0.0, -4.0],
                strength: 0.9,
                creature_id: 3,
                kind: EventKind::Vocalisation as u8,
                reserved0: [0; 3],
            }),
            Message::Derez(Derez {
                tick: 44,
                creature_id: 3,
                reserved0: [0; 4],
            }),
            Message::Ping(Ping { nonce: 0xDEAD_BEEF }),
            Message::Pong(Pong { nonce: 0xDEAD_BEEF }),
            Message::Bye,
        ]
    }

    fn frame_of(message: &Message) -> Vec<u8> {
        let mut out = Vec::new();
        encode(message, &mut out).expect("test messages are valid");
        out
    }

    #[test]
    fn every_message_survives_the_round_trip() {
        for original in everything() {
            let frame = frame_of(&original);
            let (length, type_byte) = decode_frame_header([frame[0], frame[1], frame[2]]);
            assert_eq!(length as usize, frame.len() - FRAME_HEADER_BYTES);
            let rule = payload_rule(type_byte).expect("encoded types are known");
            check_length(rule, length as usize).expect("encoded lengths are legal");
            let decoded = decode(type_byte, &frame[FRAME_HEADER_BYTES..]).expect("encoded frames decode");
            assert_eq!(decoded, original, "the wire must not launder a message into a different one");
        }
    }

    #[test]
    fn truncated_and_padded_payloads_are_refused_for_every_type() {
        for original in everything() {
            let frame = frame_of(&original);
            let type_byte = frame[2];
            let payload = &frame[FRAME_HEADER_BYTES..];

            if !payload.is_empty() {
                let truncated = &payload[..payload.len() - 1];
                assert!(decode(type_byte, truncated).is_err(), "type {type_byte} accepted a truncated payload");
            }

            let mut padded = payload.to_vec();
            padded.push(0);
            assert!(decode(type_byte, &padded).is_err(), "type {type_byte} accepted a padded payload");
        }
    }

    #[test]
    fn unknown_and_reserved_types_are_refused_before_any_copy() {
        for type_byte in [0u8, 11, 200, 255] {
            assert_eq!(payload_rule(type_byte), Err(DecodeError::UnknownOrReservedType(type_byte)));
        }
    }

    #[test]
    fn the_tick_state_length_rules_refuse_ragged_overfull_and_lying_frames() {
        let ragged = TICK_HEADER_BYTES + 7;
        assert_eq!(check_length(PayloadRule::TickState, ragged), Err(DecodeError::RaggedTickState { got: ragged }));
        assert_eq!(
            check_length(PayloadRule::TickState, TICK_HEADER_BYTES - 1),
            Err(DecodeError::RaggedTickState { got: TICK_HEADER_BYTES - 1 })
        );

        let over = TICK_HEADER_BYTES + (TICK_STATE_MAX_CREATURES as usize + 1) * CREATURE_STATE_BYTES;
        assert_eq!(
            check_length(PayloadRule::TickState, over),
            Err(DecodeError::CountOverCap {
                count: TICK_STATE_MAX_CREATURES + 1
            })
        );

        // A frame whose length carries two rows but whose header claims three: the count lies.
        let frame = frame_of(&sample_tick_state(2));
        let mut payload = frame[FRAME_HEADER_BYTES..].to_vec();
        payload[8..12].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            decode(MessageType::TickState as u8, &payload),
            Err(DecodeError::CountLengthMismatch { count: 3, rows_by_length: 2 })
        );
    }

    #[test]
    fn invalid_roles_kinds_and_nonzero_reserved_bytes_are_refused_both_ways() {
        // Decode side: corrupt genuine frames one field at a time.
        let hello = frame_of(&sample_hello());
        for bad_role in [0u8, 3, 255] {
            let mut payload = hello[FRAME_HEADER_BYTES..].to_vec();
            payload[36] = bad_role;
            assert_eq!(decode(MessageType::Hello as u8, &payload), Err(DecodeError::InvalidRole(bad_role)));
        }
        let mut payload = hello[FRAME_HEADER_BYTES..].to_vec();
        payload[38] = 1;
        assert_eq!(decode(MessageType::Hello as u8, &payload), Err(DecodeError::ReservedNotZero));

        let event = frame_of(
            everything()
                .iter()
                .find(|message| matches!(message, Message::Event(_)))
                .expect("the sample set carries an EVENT"),
        );
        let mut payload = event[FRAME_HEADER_BYTES..].to_vec();
        payload[28] = 0;
        assert_eq!(decode(MessageType::Event as u8, &payload), Err(DecodeError::InvalidEventKind(0)));

        // Encode side: the same refusals, before a byte is written.
        let mut out = Vec::new();
        let bad_role = Message::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            fingerprint: [0; 32],
            role: 9,
            reserved0: [0; 3],
            world_fingerprint: 0x5EED,
        });
        assert_eq!(encode(&bad_role, &mut out), Err(EncodeError::InvalidRole(9)));
        assert!(out.is_empty(), "a refused encode must write nothing");

        let overfull = sample_tick_state(TICK_STATE_MAX_CREATURES);
        let Message::TickState { header, mut states } = overfull else { unreachable!() };
        states.push(states[0]);
        let one_too_many = Message::TickState {
            header: TickStateHeader {
                creature_count: header.creature_count + 1,
                ..header
            },
            states,
        };
        assert_eq!(
            encode(&one_too_many, &mut out),
            Err(EncodeError::CountOverCap {
                count: TICK_STATE_MAX_CREATURES as usize + 1
            })
        );

        // A header claiming three rows over two actual ones, both under the cap: the refusal
        // names the mismatch it is, not a cap nobody exceeded.
        let lying = sample_tick_state(2);
        let Message::TickState { header, states } = lying else { unreachable!() };
        let lying_count = Message::TickState {
            header: TickStateHeader { creature_count: 3, ..header },
            states,
        };
        assert_eq!(encode(&lying_count, &mut out), Err(EncodeError::CountRowsMismatch { count: 3, rows: 2 }));
        assert!(out.is_empty(), "a refused encode must write nothing");
    }

    #[test]
    fn an_actions_reserved_word_must_be_zero_both_ways() {
        fn sample_actions() -> Message {
            Message::Actions(Actions {
                tick: 42,
                creature_id: 3,
                desired_forward_speed: 1.5,
                desired_turn_rate: -0.5,
                vocalisation_strength: 1.0,
                previous_forward_speed: 1.25,
                previous_turn_rate: 0.5,
                previous_vocalisation: 0.0,
                reserved0: [0; 4],
            })
        }

        let Message::Actions(mut actions) = sample_actions() else { unreachable!() };
        actions.reserved0 = [1, 0, 0, 0];
        let mut out = Vec::new();
        assert_eq!(encode(&Message::Actions(actions), &mut out), Err(EncodeError::ReservedNotZero));
        assert!(out.is_empty(), "a refused encode must write nothing");

        let mut frame = frame_of(&sample_actions());
        let reserved_offset = FRAME_HEADER_BYTES + 36;
        frame[reserved_offset] = 1;
        assert_eq!(decode(MessageType::Actions as u8, &frame[FRAME_HEADER_BYTES..]), Err(DecodeError::ReservedNotZero));
    }

    #[test]
    fn bye_carries_nothing_and_refuses_everything() {
        assert_eq!(check_length(PayloadRule::Exact(0), 0), Ok(()));
        assert_eq!(decode(MessageType::Bye as u8, &[]), Ok(Message::Bye));
        assert_eq!(decode(MessageType::Bye as u8, &[0]), Err(DecodeError::WrongLength { expected: 0, got: 1 }));
    }

    #[test]
    fn the_receive_buffer_constant_really_is_the_largest_legal_frame() {
        // Since protocol v4 the largest legal frame is a full REZ, not a full tick: the body
        // outweighs the world, and the receive buffer is sized by whichever is larger.
        let largest_rez = frame_of(&sample_rez(REZ_MAX_VERTICES, REZ_MAX_TRIANGLES, REZ_MAX_MATERIALS));
        let largest_tick = frame_of(&sample_tick_state(TICK_STATE_MAX_CREATURES));
        assert_eq!(largest_rez.len().max(largest_tick.len()), max_frame_bytes());
        assert!(largest_rez.len() > largest_tick.len(), "the premise: a full body outweighs a full tick");
        assert!(max_frame_bytes() <= FRAME_HEADER_BYTES + FRAME_PAYLOAD_LIMIT);
    }

    #[test]
    fn a_rez_over_any_cap_is_refused_before_a_row_is_read() {
        for (vertices, triangles, materials) in [(REZ_MAX_VERTICES + 1, 0, 0), (0, REZ_MAX_TRIANGLES + 1, 0), (0, 0, REZ_MAX_MATERIALS + 1)] {
            // Encode side: the counts are judged before the rows are even compared.
            let Message::Rez { header, .. } = sample_rez(0, 0, 0) else { unreachable!() };
            let lying = Message::Rez {
                header: Rez {
                    vertex_count: vertices,
                    triangle_count: triangles,
                    material_count: materials,
                    ..header
                },
                vertices: Vec::new(),
                triangles: Vec::new(),
                materials: Vec::new(),
            };
            let mut out = Vec::new();
            assert_eq!(encode(&lying, &mut out), Err(EncodeError::RezCountOverCap { vertices, triangles, materials }));
            assert!(out.is_empty(), "nothing is written before the refusal");

            // Decode side: a header claiming the count, with a payload of exactly header size -
            // the cap verdict comes before the length verdict, so the rows are never wanted.
            let mut payload = frame_of(&sample_rez(0, 0, 0))[FRAME_HEADER_BYTES..].to_vec();
            payload[20..24].copy_from_slice(&vertices.to_le_bytes());
            payload[24..28].copy_from_slice(&triangles.to_le_bytes());
            payload[28..32].copy_from_slice(&materials.to_le_bytes());
            assert_eq!(
                decode(MessageType::Rez as u8, &payload),
                Err(DecodeError::RezCountOverCap { vertices, triangles, materials })
            );
        }
    }

    #[test]
    fn a_rez_whose_counts_and_length_disagree_is_refused() {
        let frame = frame_of(&sample_rez(4, 2, 1));
        let payload = &frame[FRAME_HEADER_BYTES..];
        // One byte short, one byte long: the exact sum is the law.
        let short = &payload[..payload.len() - 1];
        assert!(matches!(decode(MessageType::Rez as u8, short), Err(DecodeError::RezLengthMismatch { .. })));
        let mut long = payload.to_vec();
        long.push(0);
        assert!(matches!(decode(MessageType::Rez as u8, &long), Err(DecodeError::RezLengthMismatch { .. })));
        // Shorter than the header itself is refused at header time, by the rule.
        assert!(matches!(
            check_length(PayloadRule::Rez, REZ_HEADER_BYTES - 1),
            Err(DecodeError::RezLengthMismatch { .. })
        ));
        assert!(matches!(
            check_length(PayloadRule::Rez, max_rez_payload_bytes() + 1),
            Err(DecodeError::RezLengthMismatch { .. })
        ));
        assert_eq!(check_length(PayloadRule::Rez, REZ_HEADER_BYTES), Ok(()));
    }

    #[test]
    fn a_rez_triangle_pointing_past_its_vertices_or_materials_is_refused_both_ways() {
        for (vertex, material) in [(4u32, 0u32), (0, 1), (u32::MAX, 0)] {
            let Message::Rez {
                header,
                vertices,
                mut triangles,
                materials,
            } = sample_rez(4, 2, 1)
            else {
                unreachable!()
            };
            triangles[1] = RezTriangle {
                vertices: [0, 1, vertex],
                material,
            };
            let lying = Message::Rez {
                header,
                vertices,
                triangles,
                materials,
            };
            let mut out = Vec::new();
            assert_eq!(encode(&lying, &mut out), Err(EncodeError::RezIndexOutOfRange { triangle: 1 }));

            // The same lie on the wire: patch the second triangle's bytes of an honest frame.
            let mut frame = frame_of(&sample_rez(4, 2, 1));
            let triangle_at = FRAME_HEADER_BYTES + REZ_HEADER_BYTES + 4 * size_of::<RezVertex>() + size_of::<RezTriangle>();
            frame[triangle_at + 8..triangle_at + 12].copy_from_slice(&vertex.to_le_bytes());
            frame[triangle_at + 12..triangle_at + 16].copy_from_slice(&material.to_le_bytes());
            assert_eq!(
                decode(MessageType::Rez as u8, &frame[FRAME_HEADER_BYTES..]),
                Err(DecodeError::RezIndexOutOfRange { triangle: 1 })
            );
        }
    }

    #[test]
    fn a_rez_carrying_a_nan_anywhere_is_refused_both_ways() {
        // Offsets into the payload of every float a REZ carries, by family: a bound, a vertex
        // coordinate, a material channel.
        let vertex_at = REZ_HEADER_BYTES;
        let material_at = REZ_HEADER_BYTES + 4 * size_of::<RezVertex>() + 2 * size_of::<RezTriangle>();
        for offset in [4usize, vertex_at + 4, material_at + 12] {
            for poison in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut frame = frame_of(&sample_rez(4, 2, 1));
                let at = FRAME_HEADER_BYTES + offset;
                frame[at..at + 4].copy_from_slice(&poison.to_le_bytes());
                assert_eq!(
                    decode(MessageType::Rez as u8, &frame[FRAME_HEADER_BYTES..]),
                    Err(DecodeError::RezNotFinite),
                    "offset {offset} poisoned with {poison} was accepted"
                );
            }
        }
        let Message::Rez {
            header,
            mut vertices,
            triangles,
            materials,
        } = sample_rez(4, 2, 1)
        else {
            unreachable!()
        };
        vertices[3].position[2] = f32::NAN;
        let mut out = Vec::new();
        assert_eq!(
            encode(
                &Message::Rez {
                    header,
                    vertices,
                    triangles,
                    materials
                },
                &mut out
            ),
            Err(EncodeError::RezNotFinite)
        );
    }

    /// Bombardment: deterministic junk, every length up to a whole tick-state, every type byte
    /// over a sample of them. The assertion is the doctrine itself: the codec returns, it never
    /// panics. The generator is a fixed linear congruence so a failure reproduces exactly.
    #[test]
    fn no_input_panics_the_decoder() {
        let mut seed: u64 = 0x1DA7_2026;
        let mut junk = Vec::with_capacity(max_frame_bytes());
        for length in 0..=(max_frame_bytes() - FRAME_HEADER_BYTES) {
            junk.clear();
            for _ in 0..length {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                junk.push((seed >> 56) as u8);
            }
            for type_byte in 0..=12u8 {
                let _ = decode(type_byte, &junk);
            }
            let _ = decode((seed >> 40) as u8, &junk);
        }
    }
}
