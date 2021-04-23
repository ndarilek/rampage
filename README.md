# Rampage

An audio-only, _Berzerk_-like shooter for the second [No Video Jam](https://itch.io/jam/no-video-jam-2).

## Development Setup

1. [Install Rust](https://rustup.rs). If you've got Rust installed already, be sure that it is up-to-date. This project requires _at least v1.0.51_.
2. [Install Git LFS](https://git-lfs.github.com/).
3. Either [initialize Git submodules the hard way](https://git-scm.com/book/en/v2/Git-Tools-Submodules) or clone this repository with the _--recursive_ option.
4. Under Windows, copy the DLLs in _lib/win32_ or _lib/win64_, as appropriate, to the top-level directory.
5. Under Linux, [install the dependencies documented here](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).
6. Run `cargo build` to build, and `cargo run` to build and run.

## Sound Credits

* Player death bass drop: [Talon](https://www.iamtalon.me)
* Robot footstep: [Sergenious](https://freesound.org/people/Sergenious/sounds/55846/)
* Correct exit klaxon: [jbum](https://freesound.org/people/jbum/sounds/32088/)

Other sounds were sourced from [Adobe Audition](https://www.adobe.com/products/audition/offers/AdobeAuditionDLCSFX.html) and other royalty-free libraries.