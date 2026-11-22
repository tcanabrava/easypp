# Conway's Game of Life with CUDA, GTK 4, and OpenMP

The CUDA kernel advances a 128 × 96 toroidal board. GTK 4 draws the board and
handles input. OpenMP counts live cells for the status label and computes a
CPU reference for the headless check.

Install GTK 4 and OpenMP development files, the NVIDIA CUDA Toolkit with
`nvcc` on PATH, and a working NVIDIA driver. From this directory:

```sh
easypp run --release
easypp run --release -- --check
easypp run --release -- --smoke
```

Pause to edit the board, click to toggle a cell, advance one generation with
**Step**, and restore the original seed with **Reset**. The headless check
compares the complete CUDA and OpenMP boards for 24 generations. The GTK smoke
mode opens the window, advances four generations, and exits.
