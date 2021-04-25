# Rampage

An audio-only, _Berzerk_-like shooter for the second [No Video Jam](https://itch.io/jam/no-video-jam-2).

## Gameplay

The arena consists of a grid-like maze of rooms populated by angry robots. Collisions with walls and robots are deadly, though touching a wall will make a power-up sound and give you a second to move away.

A beacon is centered in each room exit. The correct beacon emits an alarm. Follow the alarms to successfully traverse each level.

There are three robot types, from easiest to most difficult:

* Dumbasses have minimal visibility range, and don't shoot far or accurately. Level 1 is populated exclusively with dumbasses.
* Jackasses have mid-level visibility and range, and shoot more accurately. The jackass is introduced on level 2.
* Badasses have as much visibility and shot range as you do, and are far more accurate. Badasses begin to appear from level 3 onward.

Robots can sometimes shoot or collide with each other. If a robot explodes, a shockwave takes out all nearby robots after a short delay.

Killing multiple robots in a 10-second window was supposed to grant a score bonus, but I haven't implemented scoring yet. I did add the bonus system, though, so you'll receive a slightly higher tone for each robot destroyed, and a series of tones when the bonus window clears.

## Controls

| Command | Keyboard | Controller |
| Move forward | Up arrow | Left stick forward, D-pad Up |
| Move backward | Down arrow | Left stick backward, D-pad down |
| Turn left | Left arrow | Right stick left, D-pad left |
| Turn right | Right arrow | Right stick right, D-pad right |
| Strafe left | Shift left arrow | Left stick left |
| Strafe right | Shift right arrow | Left stick right |
| Snap to nearest cardinal direction left | Control left arrow | Left shoulder |
| Snap to nearest cardinal direction right | Control right arrow | Right shoulder |
| Shoot | Space | Either trigger |
| Speak coordinates | C | Left thumb |
| Speak direction in degrees | D | Right thumb |
| Speak lives remaining | H | |
| Speak current level | L | |
| Speak robots remaining | R | |
| Restart or continue to next level when prompted | Enter | Xbox A, Playstation X |
| Exit game | Escape | |

## Development Setup

1. [Install Rust](https://rustup.rs). If you've got Rust installed already, be sure that it is up-to-date. This project requires _at least v1.0.51_.
2. [Install Git LFS](https://git-lfs.github.com/).
3. Either [initialize Git submodules the hard way](https://git-scm.com/book/en/v2/Git-Tools-Submodules) or clone this repository with the _--recursive_ option.
4. Under Windows, copy the DLLs in _lib/win32_ or _lib/win64_, as appropriate, to the top-level directory.
5. Under Linux, [install the dependencies documented here](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).
6. From the top-level directory, run `cargo build` to build, and `cargo run` to build and run. Under Windows, note that you need to execute `cargo run` from the directory containing _soft_oal.dll_.

## Sound Credits

* Player death bass drop: [Talon](https://www.iamtalon.me)
* Robot footstep: [Sergenious](https://freesound.org/people/Sergenious/sounds/55846/)
* Correct exit klaxon: [jbum](https://freesound.org/people/jbum/sounds/32088/)

Other sounds were sourced from [Adobe Audition](https://www.adobe.com/products/audition/offers/AdobeAuditionDLCSFX.html) and other royalty-free libraries.