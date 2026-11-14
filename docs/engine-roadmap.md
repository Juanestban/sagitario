# The plan of a game engine in Sagitario

Rationale: [ADR-0084](adr/0084-a-complete-language-and-the-engine-apart.md), Q538. The game engine
is a project apart from the language: its own repository of Nova packages written in Sagitario.
This plan is kept here, as [ADR-0063](adr/0063-distribution-phase-and-roadmap-v2.md) wrote it,
until that repository exists and takes it. It changes nothing in the language. What only the
engine needs, such as memory for frames (pools, handles, arenas), an entity-component system or a
render interface, is built in the engine; the language takes up what the engine finds missing
only if a general-purpose language would have it (Q537).

Its first phase, G1, the prerequisites in the language, is done and closed
([roadmap-v2.md](roadmap-v2.md#g1-language-prerequisites-closed)).

## Principles

- **The engine is Sagitario packages**, built and shared with Nova: `engine-math`,
  `engine-window`, `engine-gpu`... A game depends on the ones it uses.
- **C libraries through the FFI where they are the standard way to the system**: Vulkan, OpenGL,
  the windowing APIs of each system. Whether to write the rest in Sagitario or bind a C library
  (SDL, GLFW, miniaudio, stb, Box2D) is decided per phase: binding is faster, writing it proves
  and improves the language.
- **Desktop first** (Windows, Linux, macOS), then Android and iOS, whose targets the compiler
  already has (docs/nova.md#targets). Consoles are out of scope.
- **Every phase ends in a runnable sample** in the repository, checked by CI where a GPU is not
  needed.

## G2: Windows, input and time (`engine-window`)

- Creating windows and running their event loop: Win32 on Windows, Wayland and X11 on Linux,
  Cocoa on macOS (through the C API of the Objective-C runtime).
- Keyboard, mouse, text input, gamepads; resizing, full screen and high-DPI scaling.
- A frame clock with fixed and variable time steps.
- Question: native APIs per system, or SDL3 or GLFW through the FFI first. Recommended: SDL3 or
  GLFW to get to graphics sooner, native backends later.

**Acceptance:** a window on each desktop system reports keyboard, mouse and gamepad events and
its size, at a steady frame rate.

## G3: Graphics (`engine-gpu`)

- **OpenGL** (3.3 core or 4.x): loading its functions, buffers, textures, shaders, framebuffers.
- **Vulkan**: instance, devices, swapchain, command buffers, synchronization, memory allocation,
  pipelines, descriptor sets; macOS through MoltenVK.
- **Shaders**: GLSL compiled to SPIR-V by the existing tools (`glslangValidator`, `shaderc`)
  from Nova builds.
- **A render interface** over both backends, so the engine draws without knowing which one runs.
- Question: Metal on Apple systems directly, or MoltenVK only. Recommended: MoltenVK first.

**Acceptance:** the same sample draws a textured, lit, rotating model at the frame rate of the
display with OpenGL and with Vulkan on Windows, Linux and macOS.

## G4: Engine core (`engine-core`, `engine-math`, `engine-assets`)

- **Math**: vectors, matrices, quaternions, transforms, bounding volumes, frustum culling.
- **Entities**: an entity-component-system, with scenes and a hierarchy of transforms.
- **Assets**: images (PNG, JPEG), models (glTF 2.0), fonts (TrueType), with a cache, loading on
  other threads, and reloading when files change.
- **Rendering**: 2D sprites with batching, text, cameras, lights, materials, then physically
  based rendering and shadows.
- Question: decoders written in Sagitario, or C libraries (stb_image, cgltf, stb_truetype)
  through the FFI. Recommended: C libraries first, Sagitario ones where they test the language.

**Acceptance:** a 2D sample (sprites, text, a tile map) and a 3D sample (a glTF scene with
lights and a moving camera) built from scenes and entities.

## G5: Audio (`engine-audio`)

- Output devices: WASAPI on Windows, PulseAudio and ALSA on Linux, Core Audio on macOS.
- Mixing voices with volume, panning and pitch; streaming music.
- Decoding WAV, Ogg Vorbis and MP3.
- Question: native backends, or miniaudio through the FFI. Recommended: miniaudio first.

**Acceptance:** a sample plays music and positional sound effects without gaps on the three
desktop systems.

## G6: Physics (`engine-physics`)

- Collision detection: shapes, broad phase, narrow phase, ray casts.
- Rigid bodies in 2D and 3D: integration, contacts, joints, sleeping.
- Question: written in Sagitario, or Box2D (C) and Jolt (C++, behind a C interface) through the
  FFI.

**Acceptance:** stacks of boxes and a ragdoll settle and react to the player at a fixed time
step, the same on every system.

## G7: Networking (`std::net`, `engine-net`)

- **`std::net`**: TCP and UDP sockets, addresses, name resolution, with errors as values; with
  `async` (F8) or with threads.
- **HTTP and TLS**, for downloading assets and calling services (also used by
  `nova update self`, which may start with `curl`).
- **Game networking**: messages over UDP with reliable and unreliable channels, ordering,
  fragmentation, connection and timeouts; client-server with prediction and reconciliation;
  serialization of game state.

**Acceptance:** two instances of a sample play together over the network, on the same machine
and on two machines, through packet loss and latency.

## G8: Tools (`engine-editor`)

- An immediate-mode user interface for tools (written in Sagitario, or Dear ImGui through
  `cimgui`).
- A scene editor: placing entities, editing components, saving scenes, playing in the editor.
- Profiling of frames (CPU and GPU times) and memory.
- An asset pipeline in Nova: shaders compiled, textures compressed, assets packed for release.

**Acceptance:** a level of the 3D sample is built and played from the editor, and a release
build packs its assets.

## G9: Shipping games

- Packaging a game for each desktop system, with its assets and the libraries it needs, using
  the releases of F7.
- Android and iOS: the window, input, audio and graphics layers on mobile, and their packaging
  (APK, Xcode project).

**Acceptance:** a sample game is installed and played on Windows, Linux, macOS, Android and
iOS from packages the build makes.
