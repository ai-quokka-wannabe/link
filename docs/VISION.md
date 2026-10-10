# Vision

Link is the wire of the Grid: the one protocol library that Master Control and every TronGrid Lite
instance load as the same shared binary, Rust behind a plain C ABI. It carries what the world says,
refuses what does not belong, and decides nothing about what the Grid is for. That is the
organisation's to say, and it says it below, in words mirrored across all its repositories and on
its [landing page](https://github.com/ai-quokka-wannabe); the wire's part in the arc follows.

<!-- The Long Arc: mirrored verbatim in every ai-quokka-wannabe repository (profile/README.md in .github and each docs/VISION.md). Change all five together. -->

## The Long Arc

Picture the Grid fully lit: one persistent world of glass and neon on infinite black, where AI creatures live out
their lives, crawling its terraces and calling into its echoes, and where human Users enter with avatars to meet,
and interact with, the AI animals. Something MMORPG-like: the digital frontier, built not as a film set but as a
running system. That world does not exist yet. It is where this organisation is headed, on a multi-year arc.

### The World It Becomes

- **Every creature on its own client.** Each AI creature runs its own
    [TronGrid Lite](https://github.com/ai-quokka-wannabe/tron-grid-lite) client: a creature host for its Program.
- **Every User on theirs.** Each human runs a client of their own, and enters the Grid with an avatar.
- **One world, held by Master Control.** The server holds the Grid's world state and keeps every client in sync.
- **A level editor** for authoring any Grid shape, and any placement in it.

### Real Minds, Not Masks

The creatures are the point, and the aim for them is as high as an animal goes: not simulations of animals, but
feeling beings, living in the Grid the way real animals live in the world. Not a large language model in an animal
costume, narrating what a worm might feel, but embodied animal intelligence that earns everything it knows through
its own body, behaves as realistically as a real animal would and, eventually, is as biologically realistic in its
sentience as it can be made: true animal AGI. That is the one ceiling, and it is the animal's own: animal-level
minds, never human-level ones. Below it there is no house style for brains. A brain is a Program behind a plain C
ABI, so whatever approach people contribute can plug in and earn its keep on the same honest senses. Those senses
are kept honest on purpose: a mind that could read the world's answer sheet would never have to become anything.
No such mind exists yet, and nobody can promise sentience on a schedule; it is the summit this arc climbs towards.

### The Ladder

The creatures climb a ladder of senses, from the worm upwards, specified rung by rung in
[PERCEPTION.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/PERCEPTION.md): from `elegans`,
two scalar photoreceptors and no image at all, through the compound eyes of insects and a rodent's panoramic
field, up to `macropod`, a wide eye with a horizontal streak for scanning the horizon, the eye of the quokka's own
family. Even the bottom rung is a real animal's sense: a creature that can only tell light from shade can still
perform genuine taxis. That rung is named for *C. elegans*, where
[OpenWorm](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RELATED_WORK.md#openworm-and-the-first-rung-of-the-ladder)
has spent years modelling the animal cell by cell; anyone writing a worm's mind should start there.

### The Invitation

Once the Grid can host a stranger's creature, the organisation's standing goal is to reach out to the OpenWorm
community and to the other groups simulating AI animals, and
[invite their creatures to live in the Grid](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md#the-long-term-invitation).
The whole topology is built for it: a brain written elsewhere needs only the Program ABI and a host process.
Nobody has been invited yet; that waits for the trust tier.

### Where It Stands Today

The server-authoritative half already stands, on one machine for now, as
[TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md) lays out: clients
send intents, never results. [Master Control](https://github.com/ai-quokka-wannabe/master-control) ticks one world
at 32 Hz, owns its physics and its truth, and sends the whole settled world to every client each tick through
[Link, the wire of the Grid](https://github.com/ai-quokka-wannabe/link). Each creature host loads exactly one
Program, the creature's brain. The first, [rc-worm](https://github.com/ai-quokka-wannabe/rc-worm), is a glass-neon
worm, eight icosahedra joined spike to spike with a servo at every joint. Its brain, for now, is a User at
[its panel](https://github.com/ai-quokka-wannabe/rc-worm/blob/main/docs/PANEL.md) who sees only what the worm
senses, and steers how hard its wave runs and which way it bends; how far it gets is friction's answer, not a
command's. Otherwise, humans watch through the spectator window. The Grid is one stage, built in code. There are
no avatars, no level editor, no AI minds and no persistence yet, and nothing has been released.

### Baby Steps

"Let us take baby steps towards it" are the maintainer's own words for the arc. Near-term work stays one creature
at a time, one honest subsystem at a time. rc-worm, steered from its panel, is the first step on the human side:
v0 of the human client, not a throwaway. Larger steps wait behind written triggers: the security tier behind the
first connection that is not 127.0.0.1, so no stranger's machine joins before it, and persistence behind the first
world anyone regrets losing.

### What Holds, and What Is Open

At every step, a Program perceives only through
[its own rendered senses](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/PROGRAM_INTERFACE.md):
no entity list, no ground-truth positions, no side door, whoever else is on the Grid. The Grid stays the stage,
not the actor: it renders senses and applies actions, and does no creature's thinking for it. Still open, and
left open here rather than guessed: what an avatar is, how it senses and acts, the rules by which Users and
creatures meet, and which repository will hold the level editor. No dates are promised. Master Control opens
every run with *Greetings, Programs!* The long arc bends towards the day it has Users to greet as well. From one
worm, towards one world.

## Link's Part in the Arc

**Keeping every client in sync is already the wire's work.** "One world, held by Master Control"
travels as `TICK_STATE`: the whole settled world, from the server to every client, every tick, with
no deltas and no acks, so each tick is complete in itself and no client pieces the world together
from corrections. Every message flows its own way only, and an end that speaks the other's words is
hung up on. The wire is sized for today, not for the world the arc describes: a cap of 256
creatures a tick, where today's one-machine world is sized for a dozen. What growing past that
would take, interest management included, waits in
[TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
behind written triggers.

**One binary, as the doctrine stands today.** Master Control, the creature host and the spectator
all load the same Link library, so there is no second implementation to disagree with the first,
and a client of another contract or another world is refused at the handshake, in words a human
can read. That is the rule for every end the wire has today. What shape a User's own client takes,
and whether it speaks this wire at all, is open with the rest of the avatar.

**Every creature on its own client** is the creature host's role as it stands. A host's `REZ` puts
a body on the wire: its identity, its bounds, its servos and the model it is drawn with. Its
`ACTIONS` carry the Program's intent and the angle each servo is asked to hold. And the letter,
`PROPRIOCEPTION`, carries what that body feels (its specific force, its contacts, every joint's
angle and load) to the one host that owns it and to nobody else, and the host copies it into its
creature's senses. Beyond the letter, what a Program senses is not the wire's to hand out: sensor
layouts (eyes, ears, irradiance) stay off the wire, deliberately, and the host that owns a creature
renders its senses from the world it is sent.

**The honest senses rest on the host, not on the wire.** The world a host is sent is the whole of
it: every body's pose, every tick, so that it can render its own creature's senses. The promise
that a Program sees no ground-truth positions is kept by the host, at the Program ABI, where
nothing but senses crosses, and TOPOLOGY.md accepts trusting a host with that much only until one
of its sense-integrity triggers fires. A host operated outside the maintainers' circle of trust is
one of them, and the invitation, the same document says, will pull in the first stranger's host.
When a trigger fires, TOPOLOGY.md's mitigation ladder starts with per-host filtering of this very
broadcast, and the wire is already shaped for it: every `TICK_STATE` is staged connection by
connection, carrying whatever rows the server hands it, so a filter is Master Control's choice of
rows rather than a new message.

**Every User on theirs** is where the wire says plainly what it does not know yet. Today `HELLO`
states one of two roles. A spectator watches and never sends `ACTIONS` or `REZ` (the library
refuses both itself, at both ends), and a creature host hosts a creature. A User steering rc-worm
from its panel, v0 of the human client, reaches the world only as the intent of the worm's Program:
the panel is the Program's own window, the Program turns what it is told into servo targets, and
those travel as its host's `ACTIONS` like any Program's. Nothing on the wire says who is steering.
What a User's own client will say on the wire, if anything, is open, because the arc leaves open
what an avatar is and how it senses and acts: whether it needs a role of its own, what it sends and
what it is sent are not guessed here. Whatever the answer, it lands in TOPOLOGY.md first, because
this repository implements the wire and never decides what it means.

**The steps between have written triggers on the wire too.** The server half listens on 127.0.0.1
only while the trust stance holds, and the first connection that is not 127.0.0.1 is the trigger
the deferred security tier waits behind. TCP carries every frame today, and `ACTIONS` already
resends the previous tick's intent whole: redundant on TCP, load-bearing the day the UDP trigger
fires. The wire carries the Grid's ground only as the fingerprint of its world definition, ten
numbers (a procedural, terraced floor, the tick length and the height a body stands at) that every
citizen of one world must match to the bit. The level editor has no trigger yet: what an authored
Grid would put on the wire, and which repository will hold the editor, are both open.

## What Exists Today

The contract of record is [`include/lnk/lnk_protocol.h`](../include/lnk/lnk_protocol.h): twelve
messages as no-padding plain old data, fingerprinted, with the codec refusing by name in both
directions, decode and encode alike. The built library exports one symbol, `lnkGetClientVTable`
([`include/lnk/lnk_client.h`](../include/lnk/lnk_client.h)), behind which live the dialling client,
the listening server half and the Disk: a recorder whose socket is a file and a replayer that reads
it back, so a world's life is replayed by the very code that heard it. A Disk is a recording of
what the world said, not persistence; the world outliving its server still waits behind its own
trigger. Link is Rust, `std` only, zero third-party crates, with `unsafe` confined to the C
boundary. Its consumers are Master Control, which holds the server half, and TronGrid Lite, in both
of today's client roles. Everything runs on one machine, CI builds and tests it on Windows and
Linux, and nothing has been released: the crate is version 0.0.0 and the
[CHANGELOG](../CHANGELOG.md) has only an Unreleased section.

The Grid's own vision (what the stage is, what it owes the Programs that live on it, and how it
looks) is the flagship's
[docs/VISION.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/VISION.md).
