/*
    Link - the wire of the Grid.

    This header is the Link protocol's contract of record: the framing constants, the message
    types and the payload layouts that Master Control and every TronGrid Lite instance agree on
    by loading the same library. The design authority for what these messages mean - the tick
    lifecycle, the trust stance, the deferred list - is docs/TOPOLOGY.md in the tron-grid-lite
    repository; this header only pins the bytes.

    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/link>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

/*
    Rules, shared with the flagship's Program ABI and kept for the same reasons:

    - C17 on the C side, C++11 or later on the C++ side. There is deliberately no older fallback.
    - Every struct is plain old data with no padding, and the asserts at the bottom refuse a
      layout that grows any. A member is never reordered, resized or removed without bumping
      LNK_PROTOCOL_VERSION - the fingerprint tool in tools/ refuses the change otherwise.
    - Integers and floats cross the wire little-endian, exactly as an x86-64 host lays them out;
      a big-endian consumer must swap, and the Rust side refuses to compile big-endian rather
      than pretend. Nothing here is text; nothing here is negotiated.
    - Zero is never a valid identity: message type 0, role 0 and event kind 0 are all invalid,
      so a zeroed buffer read as a message refuses loudly instead of meaning something.
*/

#ifndef LNK_PROTOCOL_H
#define LNK_PROTOCOL_H

#include <stdint.h>

#ifdef __cplusplus
extern "C"
{
#endif

/*! Bumped whenever any declaration in this header changes meaning or layout. The handshake
    carries the header's fingerprint rather than this number, so two ends disagreeing about the
    bytes are refused even when they agree about the number; the number exists for the
    human-readable refusal. */
#define LNK_PROTOCOL_VERSION 3u

/*! The port Master Control listens on when nobody names another - a default and only a
    default: the client's positional argument carries any host and port, and Master Control
    takes its own listening choice the same way. The number is the owner's: 30702, from
    JA-307020, Tron's program designation in the 1982 film - the port is the doorway into the
    Grid, and Tron is the security program who guards the system. */
#define LNK_DEFAULT_PORT 30702u

/*! Keepalive is a contract with numbers, published here exactly as the port is, because a
    timeout only works when both ends compile in the same one. An end that has heard nothing for
    LNK_KEEPALIVE_PING_MILLIS sends a PING; an end that has heard nothing for
    LNK_KEEPALIVE_DEAD_MILLIS declares the peer dead and closes the connection - LAN-sane
    figures inside the shipped norms (Gaffer's 5 s, Source's 30 s, Minecraft's 20-30 s). The
    library carries the constants and the obligation; the caller owns the clock, because a
    protocol library that owns timers is a protocol library that owns threads. */
#define LNK_KEEPALIVE_PING_MILLIS 1000u
#define LNK_KEEPALIVE_DEAD_MILLIS 10000u

/*
    Framing.

    A frame on the wire is: uint16_t payload length, then uint8_t message type, then exactly
    length bytes of payload - three bytes of header, little-endian. There is deliberately no
    struct for the header: a C struct holding a uint16_t and a uint8_t is padded to four bytes,
    and a layout that exists only with the padding suppressed is a trap for every consumer.
    Read and write the three bytes as bytes.

    The receiver validates length against the expected size for the type BEFORE copying or
    allocating anything. For every fixed-size message the length must EQUAL the payload struct's
    size - not merely fit under a maximum, so a truncated or padded frame is refused rather than
    partially read. The two variable-size messages (TICK_STATE now, REZ when its layout lands)
    state their own length rule beside their structs.
*/

/*! The first four bytes a client ever sends: 'L' 'N' 'K' '1' on the wire. Anything else earns a
    refusal and a closed connection before any frame is read.

    The handshake, in full: the client sends the magic, then a HELLO frame; the server answers
    with a WELCOME frame and the connection is framed from then on. A server that refuses - bad
    magic, wrong fingerprint, invalid role - sends a short UTF-8 line ending in '\n' and closes.
    The refusal is text rather than a frame, deliberately: it happens before the two ends have
    agreed they speak the same frames, which is exactly when a frame could not be trusted. A
    client therefore treats bytes received after its HELLO as either a WELCOME frame or, if the
    connection then closes, a refusal to put in its log verbatim. */
#define LNK_PROTOCOL_MAGIC_BYTE_0 0x4Cu
#define LNK_PROTOCOL_MAGIC_BYTE_1 0x4Eu
#define LNK_PROTOCOL_MAGIC_BYTE_2 0x4Bu
#define LNK_PROTOCOL_MAGIC_BYTE_3 0x31u

/*! Bytes of frame header on the wire: two of length, one of type. */
#define LNK_FRAME_HEADER_BYTES 3u

/*! The framing's own ceiling: length is a uint16_t, so no payload can exceed this, and every
    per-message rule below is checked against it by the asserts at the bottom. */
#define LNK_FRAME_PAYLOAD_LIMIT 65535u

/*
    Message types. One byte on the wire. Type 0 is invalid, deliberately.

    REZ's payload layout is not in this header yet: it must flatten TglCreatureDesc - whose eye
    and ear descriptors carry nested arrays - and that flattening is designed against the
    flagship's validator as its first consumer. The type number is reserved here so that nothing
    else ever takes it; a v1 end that receives REZ refuses it as unknown, which is the honest
    behaviour for a message it cannot yet parse.
*/
#define LNK_MSG_HELLO 1u
#define LNK_MSG_WELCOME 2u
#define LNK_MSG_REZ 3u
#define LNK_MSG_TICK_STATE 4u
#define LNK_MSG_ACTIONS 5u
#define LNK_MSG_EVENT 6u
#define LNK_MSG_DEREZ 7u
#define LNK_MSG_PING 8u
#define LNK_MSG_PONG 9u
#define LNK_MSG_BYE 10u

/*! What a client is, stated in HELLO. Zero is invalid. A spectator never sends ACTIONS, and the
    refusal is enforced inside this library on both ends - the sending half refuses to stage
    ACTIONS on a spectator connection, and the server half treats an ACTIONS frame arriving on
    one as a protocol violation and closes it. SourceTV's observers-fall-out-for-free, and the
    CS:GO coaching bug's lesson that spectator privilege is enforced where the authority lives,
    both as one mechanism that cannot drift because there is only one implementation. */
#define LNK_ROLE_SPECTATOR 1u
#define LNK_ROLE_CREATURE_HOST 2u

/*! Event kinds. Zero is invalid. Events are tick-stamped notifications and never load-bearing
    state: a client that misses one has missed a sound, not the world. */
#define LNK_EVENT_VOCALISATION 1u

/*! HELLO, client to server, the first frame after the magic. The fingerprint is the raw SHA-256
    of this header as tools/check_protocol_version.py hashes it; the server compares it against
    its own and a mismatch earns a human-readable refusal naming both versions, then a closed
    connection - refusal, not negotiation. */
typedef struct LnkHello {
    uint32_t protocol_version; /*!< LNK_PROTOCOL_VERSION as the client was built. */
    uint8_t fingerprint[32];   /*!< SHA-256 of this header, raw bytes, from the fingerprint tool. */
    uint8_t role;              /*!< LNK_ROLE_SPECTATOR or LNK_ROLE_CREATURE_HOST. */
    uint8_t reserved0[3];      /*!< Always zero. Named so the asserts can count it. */
} LnkHello;

/*! WELCOME, server to client, the acceptance of a HELLO. After it the server sends the REZ of
    every live creature and then the next TICK_STATE - late join is not a special case. */
typedef struct LnkWelcome {
    uint64_t current_tick;     /*!< The tick the next TICK_STATE will carry or exceed. */
    float nominal_dt_seconds;  /*!< Seconds per tick, the same number TglLibraryInfo carries. */
    uint32_t client_id;        /*!< The server's name for this connection, echoed nowhere yet. */
} LnkWelcome;

/*! One creature's row in a TICK_STATE: pose, velocity and actuator, tick-stamped by the frame's
    header struct. Forty bytes, which is what makes a dozen creatures ~500 bytes per tick. */
typedef struct LnkCreatureState {
    uint32_t creature_id;   /*!< Stable for the creature's whole rez-to-derez life. */
    float position[3];      /*!< Metres, world space, right-handed, Y up. */
    float yaw;              /*!< Radians about +Y, right-handed - the roster's own convention. */
    float velocity[3];      /*!< Metres per second, world space. */
    float yaw_rate;         /*!< Radians per second about +Y. */
    float vocalisation;     /*!< The voice actuator as physics settled it, 0 when silent. */
} LnkCreatureState;

/*! TICK_STATE, server to every client, every tick: the whole settled world, no deltas, no acks.
    The payload is this header followed immediately by creature_count LnkCreatureState rows, and
    the frame length must equal sizeof(LnkTickStateHeader) + creature_count * sizeof(LnkCreatureState)
    with creature_count <= LNK_TICK_STATE_MAX_CREATURES - both checked before any copy. */
typedef struct LnkTickStateHeader {
    uint64_t tick;          /*!< The tick these rows are the settled truth of. */
    uint32_t creature_count;
    uint8_t reserved0[4];   /*!< Always zero. Named so the asserts can count it. */
} LnkTickStateHeader;

/*! The most creatures one TICK_STATE may carry. A cap rather than a target: v1's world is a
    dozen creatures, and 256 rows is 10,256 bytes of payload against the framing's 65,535
    ceiling, so the cap can quadruple before the framing is even interesting. */
#define LNK_TICK_STATE_MAX_CREATURES 256u

/*! ACTIONS, creature host to server: the Program's staged intent for a future tick - the twelve
    bytes of TglActions plus the address, and the PREVIOUS tick's twelve piggybacked beside them,
    so one lost or late message loses nothing (Tribes repeated its moves across datagrams for
    exactly this reason; redundant on TCP, load-bearing the day the UDP trigger fires).

    The server accepts through a window, [N, N+1) against the tick N being stepped: within it the
    latest intent per creature per tick wins, deduplicated by (creature_id, tick) so the
    piggybacked copy is free to process; a stale intent loses to a newer one, and an intent
    tagged for a far future tick is refused outright rather than queued.

    Silence has authors, and the rules differ because the information differs (TOPOLOGY.md
    carries the ruling in full). A silent Program said "zero": its host sends the zeroes and the
    creature brakes, the ABI's own sentence. A silent network said nothing: the server re-applies
    the last accepted intent for up to LNK_ACTIONS_REPEAT_TICKS, then falls to zeroed coast,
    because the world must not fabricate a brake the Program never asked for. A dead host - see
    the keepalive constants - drops its creature to the zeroed neutral reflex, still embodied:
    the world never waits. */
typedef struct LnkActions {
    uint64_t tick;                 /*!< The tick this intent is staged for. */
    uint32_t creature_id;
    float desired_forward_speed;   /*!< Metres per second, clamped server-side to the body. */
    float desired_turn_rate;       /*!< Radians per second, clamped server-side to the body. */
    float vocalisation_strength;   /*!< 0 to 1, clamped server-side. */
    float previous_forward_speed;  /*!< The tick-1 intent, resent whole. Zeroes when none exists. */
    float previous_turn_rate;      /*!< See previous_forward_speed. */
    float previous_vocalisation;   /*!< See previous_forward_speed. */
    uint8_t reserved0[4];          /*!< Always zero. Named so the asserts can count it - the
                                        alternative is four bytes of invisible alignment padding,
                                        and invisible padding is exactly what this header refuses. */
} LnkActions;

/*! Ticks the server re-applies a connected host's last accepted intent when its ACTIONS are
    merely missing, before falling to zeroed coast. One: the Overwatch answer - repeat the last
    input, never stall the world, never rewind - bounded so a longer stall becomes honest
    coasting rather than a runaway. */
#define LNK_ACTIONS_REPEAT_TICKS 1u

/*! EVENT, server to every client: a tick-stamped notification, never load-bearing state. The
    spectator synthesises its audio from these; a creature host ignores them today. */
typedef struct LnkEvent {
    uint64_t tick;
    float position[3];      /*!< Where it happened, metres, world space. */
    float strength;         /*!< Kind-specific magnitude; for a vocalisation, the actuator value. */
    uint32_t creature_id;   /*!< Who caused it. */
    uint8_t kind;           /*!< LNK_EVENT_VOCALISATION. Zero is invalid. */
    uint8_t reserved0[3];   /*!< Always zero. Named so the asserts can count it. */
} LnkEvent;

/*! DEREZ, server to every client: the creature leaves the world at this tick. A leave is a
    broadcast and nothing else - late arrival is not a special case, and neither is departure. */
typedef struct LnkDerez {
    uint64_t tick;
    uint32_t creature_id;
    uint8_t reserved0[4];   /*!< Always zero. Named so the asserts can count it. */
} LnkDerez;

/*! PING, either direction. The nonce comes back verbatim in a PONG, so each end measures its
    own round trip without trusting the other's clock. */
typedef struct LnkPing {
    uint64_t nonce;
} LnkPing;

/*! PONG, the answer to a PING, carrying the same nonce. */
typedef struct LnkPong {
    uint64_t nonce;
} LnkPong;

/*
    BYE has no payload - its frame is length 0 - and so has no struct. It is a courtesy, not a
    contract: either end may also simply vanish, and the other end must survive that just as
    well, because a power cut sends no BYE.
*/

/*
    The asserts. Sum-of-members equals sizeof, so no struct has grown padding; exact sizes are
    pinned so a member cannot silently change width; and every fixed payload fits the framing.
    The same numbers are asserted again on the Rust side, so the two languages cannot drift
    apart without one of them refusing to build.
*/

#if defined(__cplusplus)
    #define LNK_STATIC_ASSERT(condition, message) static_assert(condition, message)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
    #define LNK_STATIC_ASSERT(condition, message) _Static_assert(condition, message)
#else
    #error "The Link protocol requires C17 or later (C++11 or later on the C++ side). There is deliberately no pre-C11 fallback."
#endif

#define LNK_MEMBER_BYTES(type, member) sizeof(((type*)0)->member)

LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkHello, protocol_version) + LNK_MEMBER_BYTES(LnkHello, fingerprint) + LNK_MEMBER_BYTES(LnkHello, role)
                          + LNK_MEMBER_BYTES(LnkHello, reserved0)
                      == sizeof(LnkHello),
                  "LnkHello has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkWelcome, current_tick) + LNK_MEMBER_BYTES(LnkWelcome, nominal_dt_seconds) + LNK_MEMBER_BYTES(LnkWelcome, client_id)
                      == sizeof(LnkWelcome),
                  "LnkWelcome has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkCreatureState, creature_id) + LNK_MEMBER_BYTES(LnkCreatureState, position) + LNK_MEMBER_BYTES(LnkCreatureState, yaw)
                          + LNK_MEMBER_BYTES(LnkCreatureState, velocity) + LNK_MEMBER_BYTES(LnkCreatureState, yaw_rate)
                          + LNK_MEMBER_BYTES(LnkCreatureState, vocalisation)
                      == sizeof(LnkCreatureState),
                  "LnkCreatureState has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkTickStateHeader, tick) + LNK_MEMBER_BYTES(LnkTickStateHeader, creature_count)
                          + LNK_MEMBER_BYTES(LnkTickStateHeader, reserved0)
                      == sizeof(LnkTickStateHeader),
                  "LnkTickStateHeader has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkActions, tick) + LNK_MEMBER_BYTES(LnkActions, creature_id) + LNK_MEMBER_BYTES(LnkActions, desired_forward_speed)
                          + LNK_MEMBER_BYTES(LnkActions, desired_turn_rate) + LNK_MEMBER_BYTES(LnkActions, vocalisation_strength)
                          + LNK_MEMBER_BYTES(LnkActions, previous_forward_speed) + LNK_MEMBER_BYTES(LnkActions, previous_turn_rate)
                          + LNK_MEMBER_BYTES(LnkActions, previous_vocalisation) + LNK_MEMBER_BYTES(LnkActions, reserved0)
                      == sizeof(LnkActions),
                  "LnkActions has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkEvent, tick) + LNK_MEMBER_BYTES(LnkEvent, position) + LNK_MEMBER_BYTES(LnkEvent, strength)
                          + LNK_MEMBER_BYTES(LnkEvent, creature_id) + LNK_MEMBER_BYTES(LnkEvent, kind) + LNK_MEMBER_BYTES(LnkEvent, reserved0)
                      == sizeof(LnkEvent),
                  "LnkEvent has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkDerez, tick) + LNK_MEMBER_BYTES(LnkDerez, creature_id) + LNK_MEMBER_BYTES(LnkDerez, reserved0) == sizeof(LnkDerez),
                  "LnkDerez has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkPing, nonce) == sizeof(LnkPing), "LnkPing has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkPong, nonce) == sizeof(LnkPong), "LnkPong has padding: a member changed width.");

LNK_STATIC_ASSERT(sizeof(LnkHello) == 40u, "LnkHello must be 40 bytes: version, fingerprint, role, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkWelcome) == 16u, "LnkWelcome must be 16 bytes: tick, dt, client id.");
LNK_STATIC_ASSERT(sizeof(LnkCreatureState) == 40u, "LnkCreatureState must be 40 bytes: id, pose, velocity, voice.");
LNK_STATIC_ASSERT(sizeof(LnkTickStateHeader) == 16u, "LnkTickStateHeader must be 16 bytes: tick, count, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkActions) == 40u,
                  "LnkActions must be 40 bytes: tick, id, TglActions' twelve, the previous tick's twelve resent, and a counted reserved word.");
LNK_STATIC_ASSERT(sizeof(LnkEvent) == 32u, "LnkEvent must be 32 bytes: tick, place, strength, cause, kind.");
LNK_STATIC_ASSERT(sizeof(LnkDerez) == 16u, "LnkDerez must be 16 bytes: tick, id, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkPing) == 8u && sizeof(LnkPong) == 8u, "LnkPing and LnkPong must be 8 bytes: the nonce.");

LNK_STATIC_ASSERT(sizeof(LnkTickStateHeader) + LNK_TICK_STATE_MAX_CREATURES * sizeof(LnkCreatureState) <= LNK_FRAME_PAYLOAD_LIMIT,
                  "A full TICK_STATE must fit one frame: shrink LNK_TICK_STATE_MAX_CREATURES or redesign the framing.");

#ifdef __cplusplus
}
#endif

#endif /* LNK_PROTOCOL_H */
