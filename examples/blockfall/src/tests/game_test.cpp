#include "check/check.hpp"
#include "game/game.hpp"

#include <cstdint>

using namespace blockfall;

namespace {

// The first seed whose first piece is `shape`.
std::uint32_t seed_starting_with(Shape shape, const Board& start = {})
{
    for (std::uint32_t seed = 0;; ++seed) {
        if (Game(seed, start).current().shape == shape) {
            return seed;
        }
    }
}

void hard_drop_scores_and_spawns()
{
    Game game(7);
    const Piece first = game.current();
    const Piece landing = game.ghost();
    game.handle(Input::HardDrop);
    CHECK(game.score() == 2 * (landing.y - first.y));
    CHECK(game.current().y == 0); // a fresh piece at the top
    int filled = 0;
    for (int y = 0; y < board_height; ++y) {
        for (int x = 0; x < board_width; ++x) {
            filled += game.board().at(x, y).has_value() ? 1 : 0;
        }
    }
    CHECK(filled == 4);
}

void line_clear_scores_by_level()
{
    // Bottom row full except where a flat I lands (columns 3..6).
    Board start;
    for (int x = 0; x < board_width; ++x) {
        if (x < 3 || x > 6) {
            start.set(x, board_height - 1, Shape::J);
        }
    }
    Game game(seed_starting_with(Shape::I, start), start);
    const int drop = game.ghost().y - game.current().y;
    game.handle(Input::HardDrop);
    CHECK(game.lines() == 1);
    CHECK(game.score() == 100 + 2 * drop);
    for (int x = 0; x < board_width; ++x) {
        CHECK(!game.board().at(x, board_height - 1));
    }
}

void gravity_and_lock_delay()
{
    Game game(3);
    const int y = game.current().y;
    game.tick(game.gravity_interval() * 2.5);
    CHECK(game.current().y == y + 2);

    // Resting on the floor: the piece locks only after the lock delay.
    const Piece landing = game.ghost();
    while (game.current().y < landing.y) {
        game.handle(Input::SoftDrop);
    }
    game.tick(0.3);
    CHECK(game.current().y == landing.y);
    game.tick(0.3);
    CHECK(game.current().y == 0);
}

void walls_block_movement()
{
    Game game(11);
    for (int i = 0; i < 20; ++i) {
        game.handle(Input::Left);
    }
    const int x = game.current().x;
    game.handle(Input::Left);
    CHECK(game.current().x == x);
    CHECK(game.board().fits(game.current()));
}

void hold_once_per_piece()
{
    Game game(5);
    const Shape first = game.current().shape;
    const Shape next = game.upcoming().front();
    game.handle(Input::Hold);
    CHECK(game.held() == first);
    CHECK(game.current().shape == next);
    game.handle(Input::Hold); // ignored until the piece locks
    CHECK(game.held() == first);
    game.handle(Input::HardDrop);
    const Shape now = game.current().shape;
    game.handle(Input::Hold); // swaps with the held piece
    CHECK(game.current().shape == first);
    CHECK(game.held() == now);
}

void game_over_when_blocked()
{
    Board full_top;
    for (int y = 0; y < 3; ++y) {
        for (int x = 0; x < board_width; ++x) {
            full_top.set(x, y, Shape::Z);
        }
    }
    Game game(1, full_top);
    CHECK(game.over());
    game.handle(Input::HardDrop); // input is ignored
    CHECK(game.score() == 0);
}

void levels_speed_up()
{
    Game game(9);
    const double level_one = game.gravity_interval();
    CHECK(level_one > 0.7 && level_one <= 0.8);
}

}

int main()
{
    hard_drop_scores_and_spawns();
    line_clear_scores_by_level();
    gravity_and_lock_delay();
    walls_block_movement();
    hold_once_per_piece();
    game_over_when_blocked();
    levels_speed_up();
    return 0;
}
