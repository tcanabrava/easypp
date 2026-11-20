#include "simulation.hpp"

#include <chrono>

namespace life {

using Clock = std::chrono::steady_clock;

Simulation::Simulation(int width, int height)
    : current_(width, height)
    , work_(width, height)
    , next_(width, height)
    , worker_([this] { loop(); })
{
}

Simulation::~Simulation()
{
    {
        std::lock_guard lock(mutex_);
        quit_ = true;
    }
    wake_.notify_all();
    worker_.join();
}

void Simulation::set_running(bool running)
{
    {
        std::lock_guard lock(mutex_);
        running_ = running;
        changed_ = true;
    }
    wake_.notify_all();
}

bool Simulation::running() const
{
    std::lock_guard lock(mutex_);
    return running_;
}

void Simulation::step_once()
{
    {
        std::lock_guard lock(mutex_);
        ++pending_steps_;
        changed_ = true;
    }
    wake_.notify_all();
}

void Simulation::set_speed(double generations_per_second)
{
    {
        std::lock_guard lock(mutex_);
        speed_ = generations_per_second > 0.1 ? generations_per_second : 0.1;
        changed_ = true;
    }
    wake_.notify_all();
}

void Simulation::read(Grid& out) const
{
    std::lock_guard lock(mutex_);
    out = current_;
}

void Simulation::edit(const std::function<void(Grid&)>& change)
{
    std::lock_guard lock(mutex_);
    change(current_);
    ++edits_;
}

std::uint64_t Simulation::generation() const
{
    std::lock_guard lock(mutex_);
    return generation_;
}

double Simulation::measured_rate() const
{
    std::lock_guard lock(mutex_);
    return measured_rate_;
}

void Simulation::loop()
{
    std::unique_lock lock(mutex_);
    auto next_tick = Clock::now();
    auto window_start = Clock::now();
    std::uint64_t window_generations = 0;
    std::uint64_t seen_edits = ~std::uint64_t {0};

    while (!quit_) {
        if (!running_ && pending_steps_ == 0) {
            measured_rate_ = 0.0;
            wake_.wait(lock, [this] { return quit_ || running_ || pending_steps_ > 0; });
            next_tick = Clock::now();
            window_start = next_tick;
            window_generations = 0;
            continue;
        }

        if (seen_edits != edits_) {
            work_ = current_;
            seen_edits = edits_;
        }
        lock.unlock();
        step(work_, next_); // the slow part, with OpenMP, while others can read
        lock.lock();
        if (seen_edits != edits_) {
            continue; // edited meanwhile: start again from the edited board
        }
        if (!running_ && pending_steps_ == 0) {
            continue; // paused meanwhile: a pause takes effect immediately
        }
        std::swap(work_, next_);
        current_ = work_;
        ++generation_;
        ++window_generations;
        if (pending_steps_ > 0) {
            --pending_steps_;
        }

        const auto now = Clock::now();
        const double window = std::chrono::duration<double>(now - window_start).count();
        if (window >= 0.5) {
            measured_rate_ = static_cast<double>(window_generations) / window;
            window_start = now;
            window_generations = 0;
        }

        if (running_ && pending_steps_ == 0) {
            next_tick += std::chrono::duration_cast<Clock::duration>(std::chrono::duration<double>(1.0 / speed_));
            if (next_tick < now - std::chrono::milliseconds(250)) {
                next_tick = now; // fell far behind: do not try to catch up
            }
            changed_ = false;
            wake_.wait_until(lock, next_tick, [this] { return quit_ || changed_; });
        }
    }
}

}
