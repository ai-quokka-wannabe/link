/*
    Link - the wire of the Grid. The client surface of the loadable library.

    This header declares what a consumer that LoadLibrary()s or dlopen()s Link can call: one
    exported symbol, lnkGetClientVTable, returning a table of functions behind which a client
    connection lives. It deliberately mirrors the flagship's tglGetProgramVTable shape - a
    version asked for, NULL returned when the library cannot satisfy it, and the table's own
    size as its first member - because that refusal has already proven itself there.

    This header is the API, not the wire. The wire contract - message layouts, framing, the
    fingerprint - is lnk_protocol.h, fingerprinted and versioned separately, and included here
    because the API speaks in its types. The API may grow without the wire moving; the runtime
    check below is what keeps a stale copy of THIS header from lying about the library beside
    it.

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

#ifndef LNK_CLIENT_H
#define LNK_CLIENT_H

#include <stdint.h>

#include "lnk_protocol.h"

#ifdef __cplusplus
extern "C"
{
#endif

/*! Bumped whenever this table or its rules change. lnkGetClientVTable refuses any other
    number, so a consumer built against a stale copy of this header is told at load time
    rather than corrupted at call time. */
#define LNK_CLIENT_ABI_VERSION 6u

/*
    Statuses. Zero is success and only success; everything else names its failure. No function
    behind the vtable ever unwinds, throws or aborts across this boundary - a panic inside the
    library is caught at the edge and reported as LNK_PANIC, the noexcept doctrine in library
    clothing.
*/
typedef int32_t LnkStatus;

#define LNK_OK 0
/*! poll: the socket holds no complete frame yet. Not an error; turn the loop and ask again. */
#define LNK_NOTHING_YET 1
/*! The far end said no during the handshake; the detail buffer carries its words verbatim. Also
    a Disk path refused before any file is touched: not ending in .disk, or climbing with '..'. */
#define LNK_REFUSED 2
#define LNK_HANDSHAKE_TIMED_OUT 3
#define LNK_PEER_CLOSED 4
/*! The peer sent something the wire contract refuses. The connection is over: a peer that
    framed one message wrongly will frame the next wrongly too. */
#define LNK_FRAME_REFUSED 5
/*! The handshake bytes were not the handshake. */
#define LNK_GARBLED 6
#define LNK_IO 7
/*! A null pointer, an unreadable string, a role that is not a role. The library refuses the
    call rather than dereferencing hope. */
#define LNK_BAD_ARGUMENT 8
/*! The library caught its own panic at the boundary. The connection is not to be trusted
    afterwards; close it. */
#define LNK_PANIC 9

/*
    "Client" in this header means a client of the library - either end of the wire loads the
    same binary through the same table. The connection type below is one live conversation
    whichever side of it you are; the server half further down is how Master Control (or a
    test playing Master Control) comes to hold such conversations.
*/

/*! A connected conversation, opaque. Everything it owns dies with close(). */
typedef struct LnkClient LnkClient;

/*! A listening Master Control (or a test playing one), opaque. Dies with close_server(). */
typedef struct LnkServer LnkServer;

/*! A received TICK_STATE: the header by value, the rows by borrow. The rows live in the
    library and stay valid until the next poll() or close() on the same client - copy them out
    if they must outlive that, exactly as the Program ABI's borrow rules already taught. */
typedef struct LnkTickStateView {
    LnkTickStateHeader header;
    const LnkCreatureState* states; /*!< creature_count rows, library-owned. */
} LnkTickStateView;

/*! A received REZ: the header by value, the rows by borrow with the same lifetime rules as the
    tick state's - valid until the next poll() or close() on the same client. */
typedef struct LnkRezView {
    LnkRez rez;
    const LnkRezVertex* vertices;    /*!< vertex_count rows, library-owned. */
    const LnkRezTriangle* triangles; /*!< triangle_count rows, library-owned. */
    const LnkRezMaterial* materials; /*!< material_count rows, library-owned. */
} LnkRezView;

/*! A received PROPRIOCEPTION: the header by value, the contacts borrowed from the library
    until the next poll on this connection, or close - exactly as the tick's rows. */
typedef struct LnkProprioceptionView {
    LnkProprioception proprioception;
    const LnkContact* contacts; /*!< contact_count rows, library-owned. */
} LnkProprioceptionView;

/*! One received message. `type` is the LNK_MSG_* that arrived and names the union member to
    read; BYE carries nothing, so the type alone says it all. This struct is API, not wire -
    it holds a pointer, so its size is the platform's business and nothing pins it. */
typedef struct LnkMessageView {
    uint8_t type;
    uint8_t reserved0[7];
    union {
        LnkWelcome welcome;
        LnkTickStateView tick_state;
        LnkEvent event;
        LnkDerez derez;
        LnkPing ping;
        LnkPong pong;
        LnkHello hello;     /*!< Only on a server-held connection; at a client it is the wrong way, and refused. */
        LnkActions actions; /*!< Likewise: a client's word. Every message flows one way, and the wire judges. */
        LnkRezView rez;     /*!< A creature entering the world, validated whole by the wire. */
        LnkProprioceptionView proprioception; /*!< The owner's letter: this body's feel this tick. */
    } as;
} LnkMessageView;

/*! The client surface. First two members mirror the Program ABI's vtable header: the size the
    library was compiled with, then the version, so a consumer checks both before calling
    anything. */
typedef struct LnkClientVTable {
    uint32_t vtable_bytes; /*!< sizeof(LnkClientVTable) as the library was compiled. */
    uint32_t abi_version;  /*!< LNK_CLIENT_ABI_VERSION as the library was compiled. */

    /*! LNK_PROTOCOL_VERSION as the library was compiled - the wire's version, for logs. */
    uint32_t (*protocol_version)(void);

    /*! The fingerprint this library's HELLO carries, for logs and for curiosity. */
    void (*protocol_fingerprint)(uint8_t out_fingerprint[32]);

    /*! The one implementation of the world fingerprint: FNV-1a over the definition's bytes in
        field order. Every citizen computes its own from its own values through this very
        function, so two ends disagreeing can only mean their *values* disagree. */
    uint64_t (*world_fingerprint)(const LnkWorldDefinition* definition);

    /*! The whole handshake: magic, HELLO with this library's own fingerprint and the caller's
        world fingerprint, WELCOME back - whose world fingerprint is compared against the
        caller's, because the skew check bites in both directions and a client must not
        perceive a world it would silently mis-place. Blocking, bounded by the timeout - the
        TCP connect itself included, then each read; a zero timeout is LNK_BAD_ARGUMENT, since
        it would wait for nothing. On success returns the client and writes the WELCOME. On failure returns NULL,
        writes the status, and writes a NUL-terminated UTF-8 line into the detail buffer - the
        server's refusal verbatim when there is one. `role` is LNK_ROLE_SPECTATOR or
        LNK_ROLE_CREATURE_HOST; `address_utf8` is host:port. */
    LnkClient* (*connect)(const char* address_utf8, uint8_t role, uint64_t world_fingerprint, uint32_t timeout_milliseconds, LnkWelcome* out_welcome,
                          LnkStatus* out_status, char* out_detail_utf8, uint32_t detail_capacity_bytes);

    /*! One complete message if the socket holds one (LNK_OK, view written), LNK_NOTHING_YET if
        it does not. Never blocks. Any other status ends the connection's useful life, the socket
        already shut: a frame the codec refuses; ACTIONS or REZ on a server-held connection whose
        HELLO said spectator, the role violation the protocol header names; and a message the
        wrong way - WELCOME, TICK_STATE, EVENT or PROPRIOCEPTION arriving at the server, HELLO or
        ACTIONS arriving at a client - since every message flows one way. All LNK_FRAME_REFUSED,
        the detail naming which. */
    LnkStatus (*poll)(LnkClient* client, LnkMessageView* out_message);

    /*! Stage the creature's intent for the next flush - the current tick's twelve bytes and the
        previous tick's twelve resent, exactly as ACTIONS on the wire. Refused with
        LNK_BAD_ARGUMENT on a connection that introduced itself as a spectator: a spectator
        never sends ACTIONS, and the refusal starts at the sending half. */
    LnkStatus (*send_actions)(LnkClient* client, const LnkActions* actions);

    /*! Stage a REZ: the header, then its counted rows read from the three arrays. Every count
        is validated against its cap, every index against its count and every float for
        finiteness BEFORE a single row is copied - the caller gets exactly the trust a stranger
        would. Bodiless is legitimate: all three counts zero, all three pointers ignored.
        Refused with LNK_BAD_ARGUMENT on a spectator connection, exactly as ACTIONS is. */
    LnkStatus (*send_rez)(LnkClient* client, const LnkRez* rez, const LnkRezVertex* vertices, const LnkRezTriangle* triangles,
                          const LnkRezMaterial* materials);

    /*! Stage a PING carrying the nonce; the answering PONG arrives through poll(). */
    LnkStatus (*send_ping)(LnkClient* client, uint64_t nonce);

    /*! Stage the PONG answering a received PING, same nonce back. */
    LnkStatus (*send_pong)(LnkClient* client, uint64_t nonce);

    /*! One coalesced write per tick: push everything staged. Writes 1 to out_everything_left
        when the buffer emptied, 0 when the socket filled and the remainder is carried - call
        again next tick. The carried remainder is bounded: a peer that has left more than a
        megabyte unread after the socket took what it would earns LNK_IO and the connection is
        over, because an unbounded buffer is an allocation the peer controls - a big batch to a
        peer that reads (a late joiner told every body) is never that. The keepalive constants
        in lnk_protocol.h usually reap such a peer first; the caller owns that clock. */
    LnkStatus (*flush)(LnkClient* client, uint8_t* out_everything_left);

    /*! Say BYE, close the socket, free everything the client owns. The pointer is dead after
        this call; a NULL is ignored. */
    void (*close)(LnkClient* client);

    /*
        The server half - the same library, the other end of the wire. Master Control is its
        eventual owner; the Grid's own tests were its first customer, because a spectator needs
        somebody to talk to and a hand-written test server would be the second implementation
        this organisation forbids.
    */

    /*! Listens on the port - 127.0.0.1 only while the trust stance holds: a world reachable
        from elsewhere is the trigger the deferred security tier waits behind. IPv4 loopback
        only, deliberately narrow: ::1 is not bound, so a consumer must dial 127.0.0.1 rather
        than a `localhost` an IPv6-preferring resolver might turn into ::1. Port 0 asks the
        operating system for any free port; server_port() answers which. `world_fingerprint` is
        this server's own, from the vtable's world_fingerprint over its LnkWorldDefinition:
        accept() compares every HELLO's against it and refuses skew in words. NULL on failure
        with the status and detail written. */
    LnkServer* (*listen)(uint16_t port, uint64_t world_fingerprint, LnkStatus* out_status, char* out_detail_utf8, uint32_t detail_capacity_bytes);

    /*! The port the server actually listens on - the answer to listen(0), and the number a
        log should print. Zero for a NULL server. */
    uint16_t (*server_port)(LnkServer* server);

    /*! One knock, if somebody knocked: accepts a pending connection and walks the whole
        handshake - magic, HELLO, the protocol fingerprint comparison, the world fingerprint
        comparison against the listener's own, refusals in words to the far end - bounded by
        the timeout - zero is LNK_BAD_ARGUMENT, judged before any knock is answered. LNK_NOTHING_YET when nobody is waiting; turn the loop and ask again. On
        success returns the conversation - poll, flush and close apply to it exactly as to a
        connected client - and writes the client's HELLO. The caller sends WELCOME itself,
        promptly: only it knows the current tick. */
    LnkClient* (*accept)(LnkServer* server, uint32_t timeout_milliseconds, LnkHello* out_hello, LnkStatus* out_status,
                         char* out_detail_utf8, uint32_t detail_capacity_bytes);

    /*! Stage the WELCOME that turns an accepted handshake into a citizen. The caller fills
        `world_fingerprint` with its own - the same value it gave listen() - because the skew
        check bites in both directions and the client verifies what it is welcomed into. */
    LnkStatus (*send_welcome)(LnkClient* connection, const LnkWelcome* welcome);

    /*! Stage a TICK_STATE: the header, then header->creature_count rows read from `states`.
        The count is validated against LNK_TICK_STATE_MAX_CREATURES before a single row is
        read, so a lying count cannot make the library read past the caller's array. */
    LnkStatus (*send_tick_state)(LnkClient* connection, const LnkTickStateHeader* header, const LnkCreatureState* states);

    /*! Stage an EVENT - tick-stamped notification, never load-bearing state. */
    LnkStatus (*send_event)(LnkClient* connection, const LnkEvent* event);

    /*! Stage a DEREZ - the creature leaves the world at this tick. */
    LnkStatus (*send_derez)(LnkClient* connection, const LnkDerez* derez);

    /*! Stage a PROPRIOCEPTION on a server-held connection - the owner's letter: the header,
        then header->contact_count rows read from `contacts`. The count is validated against
        LNK_CONTACTS_MAX before a single row is read; a zero count never touches the pointer.
        Refused with LNK_BAD_ARGUMENT on a connection this end dialled (a client never sends
        it), on a null header, and on a count with no rows behind it. */
    LnkStatus (*send_proprioception)(LnkClient* connection, const LnkProprioception* proprioception, const LnkContact* contacts);

    /*! Stop listening and free the server. Conversations already accepted from it live on;
        a NULL is ignored. */
    void (*close_server)(LnkServer* server);

    /*! A client whose socket is a file, the writing half: open a recording at `path_utf8`. The
        handle behaves as a server-held connection with no peer - every send_* stages a frame,
        flush writes them all (a file never says "later"), poll always answers LNK_NOTHING_YET,
        close writes BYE and closes the file - and a file that could not take the BYE (a full
        disk) ends without one, which a replay reads as the world ending in a crash: the truth.
        The path must end in .disk and never climb with '..' (LNK_REFUSED otherwise). The header names this build's protocol fingerprint
        and the world, tick and dt given here, exactly as a WELCOME would. Master Control feeds it
        from the same per-subscriber loop as every citizen: the state log is what was said, in
        the wire's own bytes, and a replay viewer is a spectator that opened it. */
    LnkClient* (*record_open)(const char* path_utf8, uint64_t world_fingerprint, uint64_t start_tick, float nominal_dt_seconds, uint64_t start_unix_seconds,
        LnkStatus* out_status, char* out_detail_utf8, uint32_t detail_capacity_bytes);

    /*! A client whose socket is a file, the reading half: open a recording and judge its header
        as a handshake is judged - another contract or another world is refused in the same words
        a server uses. `out_welcome` is filled as a server would fill it: the start tick, the dt,
        client id 0, the world. Then poll yields the recorded frames in order, send_* are refused
        (LNK_BAD_ARGUMENT - a replay has nobody to talk to), and the end of the file answers
        LNK_PEER_CLOSED: the recording is over. */
    LnkClient* (*replay_open)(const char* path_utf8, uint64_t world_fingerprint, LnkWelcome* out_welcome, LnkStatus* out_status, char* out_detail_utf8,
        uint32_t detail_capacity_bytes);
} LnkClientVTable;

/*! The library's one exported symbol. Returns the table when it can satisfy `abi_version`,
    NULL when it cannot - refusal, not negotiation, exactly as the Program ABI behaves. */
const LnkClientVTable* lnkGetClientVTable(uint32_t abi_version);

typedef const LnkClientVTable* (*LnkGetClientVTableFn)(uint32_t abi_version);

#ifdef __cplusplus
}
#endif

#endif /* LNK_CLIENT_H */
