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
#define LNK_PROTOCOL_VERSION 11u

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
    partially read. The three variable-size messages (TICK_STATE, REZ and PROPRIOCEPTION)
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
#define LNK_MSG_PROPRIOCEPTION 11u
#define LNK_MSG_REFUSED 12u

/*! What a client is, stated in HELLO. Zero is invalid. A spectator never sends ACTIONS or REZ,
    and the refusal is enforced inside this library on both ends - the sending half refuses to
    stage either on a spectator connection, and the server half treats such a frame arriving on
    one as a protocol violation and closes it. SourceTV's observers-fall-out-for-free, and the
    CS:GO coaching bug's lesson that spectator privilege is enforced where the authority lives,
    both as one mechanism that cannot drift because there is only one implementation. */
#define LNK_ROLE_SPECTATOR 1u
#define LNK_ROLE_CREATURE_HOST 2u

/*! Event kinds. Zero is invalid. Events are tick-stamped notifications and never load-bearing
    state: a client that misses one has missed a sound, not the world. */
#define LNK_EVENT_VOCALISATION 1u
/*! A body sliding along a face - the floor, a riser, another body - makes a sound: the scratch,
    sounded from the contact point, its strength the slip against the normal impulse (the
    exact-contacts ruling, TOPOLOGY.md § Master Control's mechanics). Footsteps are scratches. */
#define LNK_EVENT_SCRATCH 2u

/*!
    The shared simulation truth, gathered so it can be fingerprinted: the floor the physics
    collides against, the tick length, and the height a body stands at. Every citizen of one
    world must agree on these to the bit - a client whose floor disagrees with the server's
    mis-places every creature *silently*, which is why the handshake refuses the skew instead.
    Materials and sensor layouts are deliberately absent: they are perception, owned by the
    clients, and a disagreement there mis-shades a picture rather than corrupting the world.

    The fingerprint itself is computed by the loaded library (the vtable's world_fingerprint),
    so there is exactly one implementation of the hash and consumers only supply their values.
*/
typedef struct LnkWorldDefinition {
    uint32_t floor_cells;          /*!< Quads along each floor axis. */
    float floor_cell_size;         /*!< Edge length of one quad, metres. */
    float floor_height;            /*!< World Y of the lowest ground, metres. */
    float relief_amplitude;        /*!< Metres the highest ground stands above floor_height. */
    float relief_wavelength;       /*!< Metres between one landform and the next. */
    uint32_t relief_octaves;       /*!< Layers of value noise. */
    uint32_t relief_terraces;      /*!< Discrete height levels the relief snaps to. */
    uint32_t relief_seed;          /*!< Which landscape. */
    float dt_seconds;              /*!< Seconds per tick - the sacred number. */
    float body_half_height;        /*!< How far a body's origin stands above the ground, metres. */
} LnkWorldDefinition;

/*! HELLO, client to server, the first frame after the magic. The fingerprint is the raw SHA-256
    of this header as tools/check_protocol_version.py hashes it; the server compares it against
    its own and a mismatch earns a human-readable refusal naming both versions, then a closed
    connection - refusal, not negotiation. The world fingerprint is compared the same way: a
    client living on a different floor is refused in words, never welcomed into a world it
    would silently disagree with. */
typedef struct LnkHello {
    uint32_t protocol_version;  /*!< LNK_PROTOCOL_VERSION as the client was built. */
    uint8_t fingerprint[32];    /*!< SHA-256 of this header, raw bytes, from the fingerprint tool. */
    uint8_t role;               /*!< LNK_ROLE_SPECTATOR or LNK_ROLE_CREATURE_HOST. */
    uint8_t reserved0[3];       /*!< Always zero. Named so the asserts can count it. */
    uint64_t world_fingerprint; /*!< The vtable's world_fingerprint over the client's LnkWorldDefinition. */
} LnkHello;

/*! WELCOME, server to client, the acceptance of a HELLO. After it the server sends the REZ of
    every live creature and then the next TICK_STATE - late join is not a special case. The
    world fingerprint travels back too, so the *client* also refuses a server whose world it
    could not faithfully perceive - the skew check bites in both directions. */
typedef struct LnkWelcome {
    uint64_t current_tick;      /*!< The tick the next TICK_STATE will carry or exceed. */
    float nominal_dt_seconds;   /*!< Seconds per tick, the same number TglLibraryInfo carries. */
    uint32_t client_id;         /*!< The server's name for this connection, echoed nowhere yet. */
    uint64_t world_fingerprint; /*!< The vtable's world_fingerprint over the server's LnkWorldDefinition. */
} LnkWelcome;

/*
    REZ: a creature enters the world. Creature host to server at hosting time; server to every
    client on spawn and to every late joiner - late arrival is not a special case, so the same
    payload serves both directions and the server relays what it validated.

    The payload is the LnkRez header followed immediately by vertex_count LnkRezVertex rows,
    then triangle_count LnkRezTriangle rows, then material_count LnkRezMaterial rows, and the
    frame length must equal that sum exactly - counts bounds-checked against the caps below
    BEFORE any copy, every triangle's vertex indices below vertex_count, every material index
    below material_count, every float finite. This is the one variable-size client input - the
    Dark Souls III shape - so its parsing is gold-plated like the framing itself, single-pass
    and refuses whole.

    What travels of the descriptor is the slice the world needs: the bounds the server clamps
    intent against, and the contact budget. The sensor layout - eyes, ears, irradiance - stays
    host-local, deliberately: senses are computed by the host that owns the creature, and the
    day the sense spec moves server-side (the integrity ladder's last rung) it arrives as its
    own message, not as freight on this one. A model of zero triangles is a legitimate bodiless
    creature; its three counts are all zero and no rows follow.

    Since v7 the header also names the chain (the owner's ruling, 2026-08-26): how many
    segments the creature has, the head counted, and how far apart their origins sit along the
    head's path. Every segment wears this one model; the world places the trailing ones, and a
    TICK_STATE row carries their poses. The joint between two segments is authored into the
    model - a stub on each of the two spikes that meet - so the wire carries no joint geometry.
*/
typedef struct LnkRez {
    uint32_t creature_id;
    float max_forward_speed;       /*!< Metres per second - the bound the server clamps to. */
    float max_turn_rate;           /*!< Radians per second. */
    float max_vocalisation_strength; /*!< 0 to 1. */
    uint32_t max_contact_count;    /*!< The body's contact budget. */
    uint32_t vertex_count;         /*!< Rows of LnkRezVertex following this header. */
    uint32_t triangle_count;       /*!< Rows of LnkRezTriangle after the vertices. */
    uint32_t material_count;       /*!< Rows of LnkRezMaterial after the triangles. */
    uint32_t segment_count;        /*!< Segments in the chain, the head counted: 1 to LNK_SEGMENTS_MAX. */
    float segment_spacing;         /*!< Metres between consecutive segments' origins along the head's path;
                                        zero for a chain of one, strictly positive and finite otherwise. */
    float max_joint_angle;         /*!< Radians: the most a servo is asked to hold, the bound the server clamps
                                        every joint target to. Zero means the body has no servos - a bound of
                                        zero is no such actuator. Finite and non-negative, or refused. */
    float max_joint_torque;        /*!< Newton-metres: the most a servo holds with. Zero with a zero angle,
                                        positive with a positive one - refused otherwise. */
} LnkRez;

/*! One vertex position in body frame, metres. */
typedef struct LnkRezVertex {
    float position[3];
} LnkRezVertex;

/*! One triangle: three vertex indices and the material its surface wears. */
typedef struct LnkRezTriangle {
    uint32_t vertices[3];
    uint32_t material;
} LnkRezTriangle;

/*! One material, exactly the ABI's TglRenderMaterial shape: the smooth-limit model. */
typedef struct LnkRezMaterial {
    float colour[3];
    float index_of_refraction;
    float emission[3];
    float transmission;
} LnkRezMaterial;

/*! The three caps of the one variable-size client input, named exactly as the audit demanded.
    Sized so a full REZ fits one frame with room to spare, and generous against the first
    body's eight triangles. The material cap is the one most likely to be forgotten - it guards
    the shared slot space every triangle indexes into. */
#define LNK_REZ_MAX_VERTICES 1024u
#define LNK_REZ_MAX_TRIANGLES 2048u
#define LNK_REZ_MAX_MATERIALS 16u

/*! The most segments one creature's chain may have, the head counted (protocol v7). A chain is
    the head - the one rigid body physics steps - and up to seven trailing segments the world
    places along the head's recorded path, every one wearing the REZ's model; the joint between
    two segments is authored into the model itself, so the wire carries poses and nothing else.
    Eight is a worm of eight icosahedra, and eight rows of twenty bytes make a full TICK_STATE
    48,144 bytes - since v11 the larger legal frame, comfortably under the framing's ceiling. */
#define LNK_SEGMENTS_MAX 8u

/*! One trailing segment's pose: where it is and which way it faces, world space, the head's
    conventions. Twenty bytes. */
typedef struct LnkSegmentPose {
    float position[3];      /*!< Metres, world space, right-handed, Y up. */
    float yaw;              /*!< Radians about +Y, right-handed. */
    float pitch;            /*!< Radians about the segment's own right axis (+X in its frame), right-handed: positive is nose up. */
} LnkSegmentPose;

/*! One creature's row in a TICK_STATE: the head's pose, velocity and actuator, tick-stamped by
    the frame's header struct, then the chain - how many segments the creature has and the
    poses of every trailing one, in a fixed array so a row is always the same 188 bytes and every
    consumer copies rows by count. The slots beyond segment_count - 1 are zero, and a nonzero one
    is refused: the bytes of a tick are hashed and recorded, and a slot nobody means must not
    carry whatever was in memory. Every float is finite or the row is refused. */
typedef struct LnkCreatureState {
    uint32_t creature_id;   /*!< Stable for the creature's whole rez-to-derez life. */
    float position[3];      /*!< Metres, world space, right-handed, Y up. */
    float yaw;              /*!< Radians about +Y, right-handed - the roster's own convention. */
    float pitch;            /*!< Radians about the head's right axis, positive nose up; zero for a single body, which the world keeps level. */
    float velocity[3];      /*!< Metres per second, world space. */
    float yaw_rate;         /*!< Radians per second about +Y. */
    float vocalisation;     /*!< The voice actuator as physics settled it, 0 when silent. */
    uint32_t segment_count; /*!< Segments in the chain, the head counted: 1 to LNK_SEGMENTS_MAX. */
    LnkSegmentPose segments[LNK_SEGMENTS_MAX - 1u]; /*!< The trailing segments' poses, segment_count - 1 meaningful, the rest zero. */
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
    dozen creatures. Since v7 a row carries its chain and since v11 every pose its pitch, so 256
    rows is 48,144 bytes of payload against the framing's 65,535 ceiling - it fits, and it is now
    the larger legal frame, a maximal REZ being 45,616; the headroom the cap once had to
    quadruple was spent on the chain, deliberately. */
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
    float joint_targets[LNK_SEGMENTS_MAX - 1u];          /*!< Radians: the angle each servo is asked to hold, joint k
                                                              between segments k and k + 1, positive bending to the head's
                                                              left; segment_count - 1 meaningful, the rest zero. Clamped
                                                              server-side to max_joint_angle. The muscle. */
    float previous_joint_targets[LNK_SEGMENTS_MAX - 1u]; /*!< The tick-1 targets, resent whole. Zeroes when none exist. */
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
    spectator synthesises its audio from these, and a creature host lets its hosted senses
    hear the guests' calls through them. */
typedef struct LnkEvent {
    uint64_t tick;
    float position[3];      /*!< Where it happened, metres, world space. */
    float strength;         /*!< Kind-specific magnitude; for a vocalisation, the actuator value. */
    uint32_t creature_id;   /*!< Who caused it. */
    uint8_t kind;           /*!< LNK_EVENT_VOCALISATION or LNK_EVENT_SCRATCH. Zero is invalid. */
    uint8_t reserved0[3];   /*!< Always zero. Named so the asserts can count it. */
} LnkEvent;

/*! The most contacts a PROPRIOCEPTION carries - and the most a body may declare in its REZ's
    max_contact_count, since the letter must be able to carry every contact the body feels. */
#define LNK_CONTACTS_MAX 16u

/*! One contact a body felt this tick, in the BODY frame exactly as the Program ABI's TglContact
    hands it to a brain, so a host copies the letter into the senses without a rotation of its
    own: where on the body, the impulse the world delivered there, and - the exact-contacts
    ruling - the face's normal (world frame, unit), how deep the body stood past the face before
    it was stood back, and the slip: the body's velocity along the face, body frame, which is
    what a scratch is made of. Fifty-two bytes. */
typedef struct LnkContact {
    float position[3];  /*!< Metres, body frame. */
    float impulse[3];   /*!< Newton-seconds, body frame - the direction the body was pushed. */
    float normal[3];    /*!< Unit, world frame - which way the face pushes. */
    float depth;        /*!< Metres past the face before the body was stood back; zero at rest. */
    float slip[3];      /*!< Metres per second along the face, body frame. */
} LnkContact;

/*! PROPRIOCEPTION, server to the one host that owns the creature - a letter, not a broadcast
    (TOPOLOGY.md § The protocol, ruled 2026-08-22). Sent every tick after that tick's TICK_STATE,
    it carries what a spectator has no use for and a brain cannot do without: the specific force
    the body's otolith reads, whether the feet are on the ground, and the tick's contacts as
    contact_count LnkContact rows that follow this header in the same frame. A spectator never
    receives it; a server never receives it - this library's server half treats the frame
    arriving at the server as the same protocol violation as ACTIONS from a spectator, and the
    sending half refuses to stage it on a client-held connection. Ninety-six bytes. */
typedef struct LnkProprioception {
    uint64_t tick;
    uint32_t creature_id;
    uint8_t grounded;           /*!< 1 when the feet touch the ground this tick, else 0. */
    uint8_t reserved0[3];       /*!< Always zero. Named so the asserts can count it. */
    float specific_force[3];    /*!< Metres per second squared, BODY frame - what an otolith reads; {0, +9.81, 0} at rest. */
    /*! The angle each servo holds this tick, radians, joint k between segments k and k + 1 in the
        sign LnkActions::joint_targets asks in, within a turn; segment_count - 1 meaningful, the rest
        zero, all zero for a body of one segment. The encoder's reading - what the joint did, not
        what it was asked - so a Program can close its gait on what it feels. Finite always. */
    float joint_angles[LNK_SEGMENTS_MAX - 1u];
    /*! The torque each servo holds its angle with at the tick's end, newton-metres, signed in the
        angle's sense; at most the body's declared max_joint_torque in magnitude, and exactly that
        when the servo stalls. A motor's current sense, a tendon's organ: the load the joint bears,
        which no pose can yield. Same slots as the angles. Finite always. */
    float joint_torques[LNK_SEGMENTS_MAX - 1u];
    uint32_t contact_count;     /*!< Rows that follow, at most LNK_CONTACTS_MAX. */
    uint8_t reserved1[8];       /*!< Always zero: rounds the letter to its alignment. Named so the asserts can count it. */
} LnkProprioception;

/*! DEREZ, server to every client: the creature leaves the world at this tick. A leave is a
    broadcast and nothing else - late arrival is not a special case, and neither is departure. */
typedef struct LnkDerez {
    uint64_t tick;
    uint32_t creature_id;
    uint8_t reserved0[4];   /*!< Always zero. Named so the asserts can count it. */
} LnkDerez;

/*! Why a REZ was not honoured. Zero is invalid, deliberately, and the codec refuses anything it
    does not know: a refusal is named or it is not sent. */
#define LNK_REFUSED_OWNED 1u   /*!< Another host wears this identity; the REZ changed nothing. */
#define LNK_REFUSED_FULL 2u    /*!< The world could not carry one more row. */
#define LNK_REFUSED_CROWDED 3u /*!< Every spot of the spawn lattice is taken; a body is never stood on another. */
#define LNK_REFUSED_BOUNDS 4u  /*!< A declared bound, or the mesh, is outside what the world allows. */

/*! REFUSED, server to the one host whose REZ it did not honour - a letter, not a broadcast:
    the tick it was judged at, the creature the REZ named, and the reason by name. Until v8 a
    host learned of a refusal only by never hearing its body relayed; a world that refuses by
    name in its own log now says so to the one it refused. */
typedef struct LnkRefused {
    uint64_t tick;
    uint32_t creature_id;
    uint8_t reason;        /*!< One of LNK_REFUSED_*. Zero is invalid. */
    uint8_t reserved0[3];  /*!< Always zero. Named so the asserts can count it. */
} LnkRefused;

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
                          + LNK_MEMBER_BYTES(LnkHello, reserved0) + LNK_MEMBER_BYTES(LnkHello, world_fingerprint)
                      == sizeof(LnkHello),
                  "LnkHello has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkWelcome, current_tick) + LNK_MEMBER_BYTES(LnkWelcome, nominal_dt_seconds) + LNK_MEMBER_BYTES(LnkWelcome, client_id)
                          + LNK_MEMBER_BYTES(LnkWelcome, world_fingerprint)
                      == sizeof(LnkWelcome),
                  "LnkWelcome has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkWorldDefinition, floor_cells) + LNK_MEMBER_BYTES(LnkWorldDefinition, floor_cell_size)
                          + LNK_MEMBER_BYTES(LnkWorldDefinition, floor_height) + LNK_MEMBER_BYTES(LnkWorldDefinition, relief_amplitude)
                          + LNK_MEMBER_BYTES(LnkWorldDefinition, relief_wavelength) + LNK_MEMBER_BYTES(LnkWorldDefinition, relief_octaves)
                          + LNK_MEMBER_BYTES(LnkWorldDefinition, relief_terraces) + LNK_MEMBER_BYTES(LnkWorldDefinition, relief_seed)
                          + LNK_MEMBER_BYTES(LnkWorldDefinition, dt_seconds) + LNK_MEMBER_BYTES(LnkWorldDefinition, body_half_height)
                      == sizeof(LnkWorldDefinition),
                  "LnkWorldDefinition has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkRez, creature_id) + LNK_MEMBER_BYTES(LnkRez, max_forward_speed) + LNK_MEMBER_BYTES(LnkRez, max_turn_rate)
                          + LNK_MEMBER_BYTES(LnkRez, max_vocalisation_strength) + LNK_MEMBER_BYTES(LnkRez, max_contact_count)
                          + LNK_MEMBER_BYTES(LnkRez, vertex_count) + LNK_MEMBER_BYTES(LnkRez, triangle_count) + LNK_MEMBER_BYTES(LnkRez, material_count)
                          + LNK_MEMBER_BYTES(LnkRez, segment_count) + LNK_MEMBER_BYTES(LnkRez, segment_spacing) + LNK_MEMBER_BYTES(LnkRez, max_joint_angle)
                          + LNK_MEMBER_BYTES(LnkRez, max_joint_torque)
                      == sizeof(LnkRez),
                  "LnkRez has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkRezVertex, position) == sizeof(LnkRezVertex), "LnkRezVertex has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkRezTriangle, vertices) + LNK_MEMBER_BYTES(LnkRezTriangle, material) == sizeof(LnkRezTriangle),
                  "LnkRezTriangle has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkRezMaterial, colour) + LNK_MEMBER_BYTES(LnkRezMaterial, index_of_refraction)
                          + LNK_MEMBER_BYTES(LnkRezMaterial, emission) + LNK_MEMBER_BYTES(LnkRezMaterial, transmission)
                      == sizeof(LnkRezMaterial),
                  "LnkRezMaterial has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkCreatureState, creature_id) + LNK_MEMBER_BYTES(LnkCreatureState, position) + LNK_MEMBER_BYTES(LnkCreatureState, yaw)
                          + LNK_MEMBER_BYTES(LnkCreatureState, pitch)
                          + LNK_MEMBER_BYTES(LnkCreatureState, velocity) + LNK_MEMBER_BYTES(LnkCreatureState, yaw_rate)
                          + LNK_MEMBER_BYTES(LnkCreatureState, vocalisation) + LNK_MEMBER_BYTES(LnkCreatureState, segment_count)
                          + LNK_MEMBER_BYTES(LnkCreatureState, segments)
                      == sizeof(LnkCreatureState),
                  "LnkCreatureState has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkSegmentPose, position) + LNK_MEMBER_BYTES(LnkSegmentPose, yaw) + LNK_MEMBER_BYTES(LnkSegmentPose, pitch) == sizeof(LnkSegmentPose),
                  "LnkSegmentPose has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkTickStateHeader, tick) + LNK_MEMBER_BYTES(LnkTickStateHeader, creature_count)
                          + LNK_MEMBER_BYTES(LnkTickStateHeader, reserved0)
                      == sizeof(LnkTickStateHeader),
                  "LnkTickStateHeader has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkActions, tick) + LNK_MEMBER_BYTES(LnkActions, creature_id) + LNK_MEMBER_BYTES(LnkActions, desired_forward_speed)
                          + LNK_MEMBER_BYTES(LnkActions, desired_turn_rate) + LNK_MEMBER_BYTES(LnkActions, vocalisation_strength)
                          + LNK_MEMBER_BYTES(LnkActions, previous_forward_speed) + LNK_MEMBER_BYTES(LnkActions, previous_turn_rate)
                          + LNK_MEMBER_BYTES(LnkActions, previous_vocalisation) + LNK_MEMBER_BYTES(LnkActions, joint_targets)
                          + LNK_MEMBER_BYTES(LnkActions, previous_joint_targets) + LNK_MEMBER_BYTES(LnkActions, reserved0)
                      == sizeof(LnkActions),
                  "LnkActions has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkEvent, tick) + LNK_MEMBER_BYTES(LnkEvent, position) + LNK_MEMBER_BYTES(LnkEvent, strength)
                          + LNK_MEMBER_BYTES(LnkEvent, creature_id) + LNK_MEMBER_BYTES(LnkEvent, kind) + LNK_MEMBER_BYTES(LnkEvent, reserved0)
                      == sizeof(LnkEvent),
                  "LnkEvent has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkDerez, tick) + LNK_MEMBER_BYTES(LnkDerez, creature_id) + LNK_MEMBER_BYTES(LnkDerez, reserved0) == sizeof(LnkDerez),
                  "LnkDerez has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkRefused, tick) + LNK_MEMBER_BYTES(LnkRefused, creature_id) + LNK_MEMBER_BYTES(LnkRefused, reason)
                          + LNK_MEMBER_BYTES(LnkRefused, reserved0)
                      == sizeof(LnkRefused),
                  "LnkRefused has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkPing, nonce) == sizeof(LnkPing), "LnkPing has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkPong, nonce) == sizeof(LnkPong), "LnkPong has padding: a member changed width.");

LNK_STATIC_ASSERT(sizeof(LnkHello) == 48u, "LnkHello must be 48 bytes: version, fingerprint, role, reserved, world fingerprint.");
LNK_STATIC_ASSERT(sizeof(LnkWelcome) == 24u, "LnkWelcome must be 24 bytes: tick, dt, client id, world fingerprint.");
LNK_STATIC_ASSERT(sizeof(LnkWorldDefinition) == 40u, "LnkWorldDefinition must be 40 bytes: the floor's eight numbers, dt, and the standing height.");
LNK_STATIC_ASSERT(sizeof(LnkRez) == 48u, "LnkRez must be 48 bytes: identity, the bounds, the contact budget, three counts, the chain's count and spacing, the servos' angle and torque.");
LNK_STATIC_ASSERT(sizeof(LnkRezVertex) == 12u && sizeof(LnkRezTriangle) == 16u && sizeof(LnkRezMaterial) == 32u,
                  "The REZ rows must stay exactly the sizes the length rule multiplies by.");
LNK_STATIC_ASSERT(sizeof(LnkSegmentPose) == 20u, "LnkSegmentPose must be 20 bytes: position, yaw, pitch.");
LNK_STATIC_ASSERT(sizeof(LnkCreatureState) == 188u, "LnkCreatureState must be 188 bytes: id, pose with pitch, velocity, voice, the chain's count and seven poses.");
LNK_STATIC_ASSERT(sizeof(LnkTickStateHeader) == 16u, "LnkTickStateHeader must be 16 bytes: tick, count, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkActions) == 96u, "LnkActions must be 96 bytes: the tick, the identity, three actions, their tick-1 resend, seven servo targets, their tick-1 resend, and a reserved word.");
LNK_STATIC_ASSERT(sizeof(LnkEvent) == 32u, "LnkEvent must be 32 bytes: tick, place, strength, cause, kind.");
LNK_STATIC_ASSERT(sizeof(LnkDerez) == 16u, "LnkDerez must be 16 bytes: tick, id, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkRefused) == 16u, "LnkRefused must be 16 bytes: tick, id, reason, reserved.");
LNK_STATIC_ASSERT(sizeof(LnkContact) == 52u, "LnkContact must be 52 bytes: position, impulse, normal, depth, slip.");
LNK_STATIC_ASSERT(sizeof(LnkProprioception) == 96u, "LnkProprioception must be 96 bytes: tick, id, grounded, reserved, force, seven servo angles, seven servo torques, count, reserved.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkContact, position) + LNK_MEMBER_BYTES(LnkContact, impulse) + LNK_MEMBER_BYTES(LnkContact, normal)
                          + LNK_MEMBER_BYTES(LnkContact, depth) + LNK_MEMBER_BYTES(LnkContact, slip)
                      == sizeof(LnkContact),
                  "LnkContact has padding: a member changed width.");
LNK_STATIC_ASSERT(LNK_MEMBER_BYTES(LnkProprioception, tick) + LNK_MEMBER_BYTES(LnkProprioception, creature_id) + LNK_MEMBER_BYTES(LnkProprioception, grounded)
                          + LNK_MEMBER_BYTES(LnkProprioception, reserved0) + LNK_MEMBER_BYTES(LnkProprioception, specific_force)
                          + LNK_MEMBER_BYTES(LnkProprioception, joint_angles) + LNK_MEMBER_BYTES(LnkProprioception, joint_torques)
                          + LNK_MEMBER_BYTES(LnkProprioception, contact_count)
                          + LNK_MEMBER_BYTES(LnkProprioception, reserved1)
                      == sizeof(LnkProprioception),
                  "LnkProprioception has padding: a member changed width.");
LNK_STATIC_ASSERT(sizeof(LnkProprioception) + LNK_CONTACTS_MAX * sizeof(LnkContact) <= 65535u,
                  "A full PROPRIOCEPTION must fit one frame.");
LNK_STATIC_ASSERT(sizeof(LnkPing) == 8u && sizeof(LnkPong) == 8u, "LnkPing and LnkPong must be 8 bytes: the nonce.");

LNK_STATIC_ASSERT(sizeof(LnkTickStateHeader) + LNK_TICK_STATE_MAX_CREATURES * sizeof(LnkCreatureState) <= LNK_FRAME_PAYLOAD_LIMIT,
                  "A full TICK_STATE must fit one frame: shrink LNK_TICK_STATE_MAX_CREATURES or redesign the framing.");
LNK_STATIC_ASSERT(sizeof(LnkRez) + LNK_REZ_MAX_VERTICES * sizeof(LnkRezVertex) + LNK_REZ_MAX_TRIANGLES * sizeof(LnkRezTriangle)
                          + LNK_REZ_MAX_MATERIALS * sizeof(LnkRezMaterial)
                      <= LNK_FRAME_PAYLOAD_LIMIT,
                  "A maximal REZ must fit one frame: shrink a cap or redesign the framing.");

#ifdef __cplusplus
}
#endif

#endif /* LNK_PROTOCOL_H */
