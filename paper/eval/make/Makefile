CXX        := clang++
CXXFLAGS   := -std=c++20 -Wall -pedantic -Werror -O0 -g -Isrc/libs -MMD -MP
SDL_CFLAGS := $(shell pkg-config --cflags sdl2)
SDL_LIBS   := $(shell pkg-config --libs sdl2)
B          := build

obj = $(patsubst src/%.cpp,$(B)/%.o,$(1))
lib = $(B)/lib$(1).a

GAME_LIBS := $(call lib,game) $(call lib,bag) $(call lib,board) $(call lib,pieces)
TESTS     := $(B)/pieces_test $(B)/bag_test $(B)/board_test $(B)/game_test

all: $(B)/blockfall $(TESTS)

$(B)/%.o: src/%.cpp
	@mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) $(SDL_CFLAGS) -c $< -o $@

$(B)/lib%.a:
	ar rcs $@ $^

$(call lib,pieces): $(call obj,src/libs/pieces/pieces.cpp)
$(call lib,bag):    $(call obj,src/libs/bag/bag.cpp)
$(call lib,board):  $(call obj,src/libs/board/board.cpp)
$(call lib,game):   $(call obj,src/libs/game/game.cpp)
$(call lib,render): $(call obj,src/libs/render/render.cpp)

$(B)/blockfall: $(call obj,src/bin/blockfall.cpp) $(call lib,render) $(GAME_LIBS)
	$(CXX) $^ $(SDL_LIBS) -o $@

$(B)/pieces_test: $(call obj,src/tests/pieces_test.cpp) $(call lib,pieces)
$(B)/bag_test:    $(call obj,src/tests/bag_test.cpp) $(call lib,bag) $(call lib,pieces)
$(B)/board_test:  $(call obj,src/tests/board_test.cpp) $(call lib,board) $(call lib,pieces)
$(B)/game_test:   $(call obj,src/tests/game_test.cpp) $(GAME_LIBS)
$(TESTS):
	$(CXX) $^ -o $@

test: $(TESTS)
	@for t in $(TESTS); do $$t && echo "$$t ok" || exit 1; done

clean:
	rm -rf $(B)

-include $(shell find $(B) -name '*.d' 2>/dev/null)
.PHONY: all test clean
