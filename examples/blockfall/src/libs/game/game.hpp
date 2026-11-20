#pragma once

#include "bag/bag.hpp"
#include "board/board.hpp"

#include <cstdint>
#include <deque>
#include <optional>

namespace blockfall {

enum class Input { Left, Right, SoftDrop, HardDrop, RotateCw, RotateCcw, Hold };

// The rules, with no knowledge of graphics or input devices.
class Game {
public:
    explicit Game(std::uint32_t seed, Board start = {});

    void handle(Input input);
    // Advances gravity and lock delay by `seconds`.
    void tick(double seconds);

    const Board& board() const { return board_; }
    const Piece& current() const { return current_; }
    // Where the current piece would land with a hard drop.
    Piece ghost() const;
    const std::deque<Shape>& upcoming() const { return queue_; }
    std::optional<Shape> held() const { return held_; }

    int score() const { return score_; }
    int lines() const { return lines_; }
    int level() const { return level_; }
    bool over() const { return over_; }
    // Seconds per row of gravity at the current level.
    double gravity_interval() const;

private:
    Shape take_next();
    void spawn(Shape shape);
    bool try_move(int dx, int dy);
    bool try_rotate(int direction);
    bool resting() const;
    void moved_while_resting();
    void lock_piece();
    void hold();

    Bag bag_;
    Board board_;
    std::deque<Shape> queue_;
    Piece current_ {};
    std::optional<Shape> held_;
    bool can_hold_ = true;

    double gravity_timer_ = 0;
    double lock_timer_ = 0;
    int lock_resets_ = 0;

    int score_ = 0;
    int lines_ = 0;
    int level_ = 1;
    bool over_ = false;
};

}
