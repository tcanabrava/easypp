# Blockfall

An original falling-block puzzle game, used as an easypp test project:
seven internal libraries, SDL2 found and linked with no configuration, and
unit tests for the game logic that don't link SDL.

```sh
easypp run blockfall      # play
easypp test               # pieces, bag, board and game tests
easypp info               # what links to what
```

| Key | Action |
|---|---|
| Left / Right | move |
| Up or X / Z | rotate clockwise / counter-clockwise |
| Down / Space | soft drop / hard drop |
| C or Shift | hold |
| P | pause |
| R | restart after game over |
| Esc | quit |

## Layout

```text
src/libs/pieces      shapes and rotations
src/libs/bag         shuffled randomizer               (uses pieces)
src/libs/board       grid, collisions, line clears     (uses pieces)
src/libs/game        rules, gravity, scoring, hold     (uses bag, board)
src/libs/pixelfont   header-only 3x5 font              (includes <SDL2/SDL.h>)
src/libs/render      drawing                           (uses game, pixelfont, SDL2)
src/libs/check       header-only CHECK() for tests
src/bin/blockfall.cpp
src/tests/*_test.cpp
```

Headless smoke test:

```sh
SDL_VIDEODRIVER=dummy build/debug/bin/blockfall --demo --frames 600 --screenshot shot.bmp
```
