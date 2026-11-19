# Life: a GTK4 + OpenMP + std::thread smoke test for easypp

Conway's Game of Life in a GTK4 window. It is here to prove that one easypp
project can combine GTK4, OpenMP and threads without any configuration:

- `life`: each generation is computed with `#pragma omp parallel for` and a
  `reduction`. easypp sees the pragma and adds `-fopenmp`.
- `simulation`: the game runs on its own `std::thread`. It computes outside its
  mutex, so the window never waits for a generation.
- `view`: cairo drawing, used both by the window and by the headless PNG mode.
- `src/bin/life.cpp`: the GTK4 user interface.

```sh
easypp run life                                        # open the window
easypp run life -- --smoke 3 --screenshot shot.png     # play 3 s, save the window, quit
easypp run life -- --headless 2000 --png board.png     # no display needed
OMP_NUM_THREADS=1 build/debug/bin/life --headless 2000 # compare thread counts
easypp test                                            # rules, OpenMP == serial, threading
```

Window keys: Space play/pause, S or Right step, R random, G glider gun, C clear.
Click or drag on the board to draw cells.
