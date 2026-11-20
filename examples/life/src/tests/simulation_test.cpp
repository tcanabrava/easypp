#include "check/check.hpp"
#include "patterns/patterns.hpp"
#include "simulation/simulation.hpp"

#include <chrono>
#include <thread>

using namespace life;
using namespace std::chrono_literals;

namespace {

template <typename Condition> bool eventually(Condition condition)
{
    const auto deadline = std::chrono::steady_clock::now() + 5s;
    while (std::chrono::steady_clock::now() < deadline) {
        if (condition()) {
            return true;
        }
        std::this_thread::sleep_for(1ms);
    }
    return false;
}

}

int main()
{
    Simulation simulation(16, 16);
    simulation.edit([](Grid& grid) { place(grid, Pattern::Glider, 0, 0); });

    // Paused: nothing happens until asked.
    std::this_thread::sleep_for(20ms);
    CHECK(simulation.generation() == 0);
    simulation.step_once();
    CHECK(eventually([&] { return simulation.generation() == 1; }));

    // Running: the worker thread advances on its own.
    simulation.set_speed(1000);
    simulation.set_running(true);
    CHECK(eventually([&] { return simulation.generation() >= 50; }));
    simulation.set_running(false);
    const auto stopped_at = simulation.generation();
    std::this_thread::sleep_for(20ms);
    CHECK(simulation.generation() == stopped_at);

    // A glider keeps its five cells, whatever generation the thread reached.
    Grid grid(16, 16);
    simulation.read(grid);
    CHECK(grid.population() == 5);
    return 0; // the destructor stops and joins the thread
}
