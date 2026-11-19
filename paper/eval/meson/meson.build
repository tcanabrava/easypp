project('blockfall', 'cpp',
  default_options: ['cpp_std=c++20', 'warning_level=1', 'werror=true'])
add_project_arguments('-pedantic', language: 'cpp')

inc = include_directories('src/libs')
sdl2 = dependency('sdl2')

pieces = static_library('pieces', 'src/libs/pieces/pieces.cpp', include_directories: inc)
pieces_dep = declare_dependency(link_with: pieces, include_directories: inc)

bag = static_library('bag', 'src/libs/bag/bag.cpp', dependencies: pieces_dep)
bag_dep = declare_dependency(link_with: bag, dependencies: pieces_dep)

board = static_library('board', 'src/libs/board/board.cpp', dependencies: pieces_dep)
board_dep = declare_dependency(link_with: board, dependencies: pieces_dep)

game = static_library('game', 'src/libs/game/game.cpp', dependencies: [bag_dep, board_dep])
game_dep = declare_dependency(link_with: game, dependencies: [bag_dep, board_dep])

pixelfont_dep = declare_dependency(include_directories: inc, dependencies: sdl2)

render = static_library('render', 'src/libs/render/render.cpp',
  dependencies: [game_dep, pixelfont_dep])
render_dep = declare_dependency(link_with: render, dependencies: [game_dep, sdl2])

check_dep = declare_dependency(include_directories: inc)

executable('blockfall', 'src/bin/blockfall.cpp', dependencies: [render_dep, sdl2])

foreach name : ['pieces', 'bag', 'board', 'game']
  t = executable(name + '_test', 'src/tests/' + name + '_test.cpp',
    dependencies: [get_variable(name + '_dep'), check_dep])
  test(name + '_test', t)
endforeach
