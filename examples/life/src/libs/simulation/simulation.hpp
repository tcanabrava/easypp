#pragma once

#include "life/life.hpp"

#include <condition_variable>
#include <cstdint>
#include <functional>
#include <mutex>
#include <thread>

namespace life {

// Runs the game on its own std::thread, so the user interface never waits for
// a generation. Each generation is itself computed by OpenMP threads.
// All member functions are safe to call from any thread.
class Simulation {
public:
    Simulation(int width, int height);
    ~Simulation(); // stops and joins the worker thread

    Simulation(const Simulation&) = delete;
    Simulation& operator=(const Simulation&) = delete;

    void set_running(bool running);
    bool running() const;
    // Advances one generation (useful while paused).
    void step_once();
    void set_speed(double generations_per_second);

    // Copies the current board into `out`.
    void read(Grid& out) const;
    // Changes the board, e.g. to toggle a cell or load a pattern.
    void edit(const std::function<void(Grid&)>& change);

    std::uint64_t generation() const;
    // Generations per second measured over the last half second.
    double measured_rate() const;

private:
    void loop();

    // Shared state, guarded by mutex_. Generations are computed outside the
    // lock, so readers only ever wait for a copy, never for a whole step.
    mutable std::mutex mutex_;
    std::condition_variable wake_;
    Grid current_;
    std::uint64_t edits_ = 0;
    std::uint64_t generation_ = 0;
    int pending_steps_ = 0;
    double speed_ = 30.0;
    double measured_rate_ = 0.0;
    bool running_ = false;
    bool changed_ = false;
    bool quit_ = false;

    // Only touched by the worker thread.
    Grid work_;
    Grid next_;

    std::thread worker_; // last, so it starts after everything above exists
};

}
