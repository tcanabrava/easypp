// Conway's Game of Life in a GTK4 window: a smoke test for easypp with GTK4,
// OpenMP (each generation) and std::thread (the simulation runs beside the UI).
//
//   life                                   open the window
//   life --smoke 3 --screenshot shot.png   play 3 s, save the window, quit
//   life --headless 200 --png board.png    no display: 200 generations to a PNG
//
// In the window: Space play/pause, S or Right step, R random, C clear,
// G glider gun, click or drag to draw cells.

#include "patterns/patterns.hpp"
#include "simulation/simulation.hpp"
#include "view/view.hpp"

#include <gtk/gtk.h>

#include <charconv>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <random>
#include <string>
#include <thread>

namespace {

constexpr int board_width = 200;
constexpr int board_height = 130;

struct App {
    life::Simulation simulation {board_width, board_height};
    life::Grid frame {board_width, board_height}; // what the window shows
    GtkWidget* window = nullptr;
    GtkWidget* area = nullptr;
    GtkWidget* status = nullptr;
    GtkWidget* play = nullptr;
    bool draw_alive = true; // what a drag paints
    double smoke_seconds = 0;
    const char* screenshot = nullptr;
};

std::uint32_t fresh_seed()
{
    return std::random_device {}();
}

void load_start(life::Simulation& simulation, std::uint32_t seed)
{
    simulation.edit([seed](life::Grid& grid) {
        grid.clear();
        life::randomize(grid, 0.25, seed);
        life::place(grid, life::Pattern::GliderGun, 4, 4);
    });
}

// A whole command-line number, or false if the text is not one (unlike atof,
// which silently turns "abc" into 0).
template <typename T> bool parse_number(const char* text, T& out)
{
    const char* end = text + std::strlen(text);
    const auto [last, error] = std::from_chars(text, end, out);
    return error == std::errc() && last == end;
}

// ---- drawing and status -------------------------------------------------

void draw_board(GtkDrawingArea*, cairo_t* cr, int width, int height, gpointer data)
{
    auto* app = static_cast<App*>(data);
    life::view::draw(cr, app->frame, width, height);
}

void update_status(App& app)
{
    char text[200];
    std::snprintf(text, sizeof text,
        "generation %llu   ·   population %d   ·   %.0f gen/s   ·   OpenMP threads %d   ·   "
        "simulation on its own std::thread (%s)",
        static_cast<unsigned long long>(app.simulation.generation()), app.frame.population(),
        app.simulation.measured_rate(), life::threads(), app.simulation.running() ? "running" : "paused");
    gtk_label_set_text(GTK_LABEL(app.status), text);
    gtk_button_set_label(GTK_BUTTON(app.play), app.simulation.running() ? "Pause" : "Play");
}

// Runs once per frame: copy the board from the simulation thread and redraw.
gboolean on_tick(GtkWidget*, GdkFrameClock*, gpointer data)
{
    auto* app = static_cast<App*>(data);
    app->simulation.read(app->frame);
    gtk_widget_queue_draw(app->area);
    update_status(*app);
    return G_SOURCE_CONTINUE;
}

// ---- input --------------------------------------------------------------

void paint_at(App& app, double px, double py, bool toggle)
{
    int x = 0;
    int y = 0;
    const int w = gtk_widget_get_width(app.area);
    const int h = gtk_widget_get_height(app.area);
    if (!life::view::cell_at(app.frame, w, h, px, py, x, y)) {
        return;
    }
    if (toggle) {
        app.draw_alive = !app.frame.alive(x, y);
    }
    const bool alive = app.draw_alive;
    app.simulation.edit([x, y, alive](life::Grid& grid) { grid.set(x, y, alive); });
    app.frame.set(x, y, alive);
    gtk_widget_queue_draw(app.area);
}

void on_drag_begin(GtkGestureDrag*, double x, double y, gpointer data)
{
    paint_at(*static_cast<App*>(data), x, y, true);
}

void on_drag_update(GtkGestureDrag* gesture, double dx, double dy, gpointer data)
{
    double x = 0;
    double y = 0;
    gtk_gesture_drag_get_start_point(gesture, &x, &y);
    paint_at(*static_cast<App*>(data), x + dx, y + dy, false);
}

void toggle_running(App& app)
{
    app.simulation.set_running(!app.simulation.running());
}

void on_play(GtkButton*, gpointer data)
{
    toggle_running(*static_cast<App*>(data));
}

void on_step(GtkButton*, gpointer data)
{
    static_cast<App*>(data)->simulation.step_once();
}

void on_random(GtkButton*, gpointer data)
{
    static_cast<App*>(data)->simulation.edit([](life::Grid& grid) { life::randomize(grid, 0.25, fresh_seed()); });
}

void on_clear(GtkButton*, gpointer data)
{
    static_cast<App*>(data)->simulation.edit([](life::Grid& grid) { grid.clear(); });
}

void on_gun(GtkButton*, gpointer data)
{
    static_cast<App*>(data)->simulation.edit([](life::Grid& grid) {
        grid.clear();
        life::place(grid, life::Pattern::GliderGun, 4, 4);
    });
}

void on_speed(GtkRange* range, gpointer data)
{
    static_cast<App*>(data)->simulation.set_speed(gtk_range_get_value(range));
}

gboolean on_key(GtkEventControllerKey*, guint key, guint, GdkModifierType, gpointer data)
{
    auto* app = static_cast<App*>(data);
    switch (key) {
    case GDK_KEY_space:
        toggle_running(*app);
        return TRUE;
    case GDK_KEY_s:
    case GDK_KEY_Right:
        on_step(nullptr, app);
        return TRUE;
    case GDK_KEY_r:
        on_random(nullptr, app);
        return TRUE;
    case GDK_KEY_c:
        on_clear(nullptr, app);
        return TRUE;
    case GDK_KEY_g:
        on_gun(nullptr, app);
        return TRUE;
    default:
        return FALSE;
    }
}

// ---- smoke test ---------------------------------------------------------

// Renders the whole window, widgets included, to a PNG.
bool save_window(GtkWidget* window, const char* path)
{
    const int width = gtk_widget_get_width(window);
    const int height = gtk_widget_get_height(window);
    GdkPaintable* paintable = gtk_widget_paintable_new(window);
    GtkSnapshot* snapshot = gtk_snapshot_new();
    gdk_paintable_snapshot(paintable, snapshot, width, height);
    GskRenderNode* node = gtk_snapshot_free_to_node(snapshot);
    bool ok = false;
    if (node != nullptr) {
        GskRenderer* renderer = gtk_native_get_renderer(GTK_NATIVE(window));
        GdkTexture* texture = gsk_renderer_render_texture(renderer, node, nullptr);
        ok = gdk_texture_save_to_png(texture, path);
        g_object_unref(texture);
        gsk_render_node_unref(node);
    }
    g_object_unref(paintable);
    return ok;
}

gboolean finish_smoke(gpointer data)
{
    auto* app = static_cast<App*>(data);
    app->simulation.read(app->frame);
    std::printf("smoke test: %llu generations at %.0f gen/s, population %d, %d OpenMP threads\n",
        static_cast<unsigned long long>(app->simulation.generation()), app->simulation.measured_rate(),
        app->frame.population(), life::threads());
    if (app->screenshot != nullptr) {
        std::printf("%s %s\n", save_window(app->window, app->screenshot) ? "saved" : "could not save", app->screenshot);
    }
    gtk_window_destroy(GTK_WINDOW(app->window));
    return G_SOURCE_REMOVE;
}

// ---- window -------------------------------------------------------------

GtkWidget* button(const char* label, GCallback callback, App* app)
{
    GtkWidget* b = gtk_button_new_with_label(label);
    g_signal_connect(b, "clicked", callback, app);
    return b;
}

void activate(GtkApplication* gtk_app, gpointer data)
{
    auto* app = static_cast<App*>(data);

    app->window = gtk_application_window_new(gtk_app);
    gtk_window_set_title(GTK_WINDOW(app->window), "Life · easypp smoke test (GTK4 + OpenMP + std::thread)");
    gtk_window_set_default_size(GTK_WINDOW(app->window), 1000, 760);

    GtkWidget* toolbar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_widget_set_margin_start(toolbar, 8);
    gtk_widget_set_margin_end(toolbar, 8);
    gtk_widget_set_margin_top(toolbar, 8);
    app->play = button("Pause", G_CALLBACK(on_play), app);
    gtk_box_append(GTK_BOX(toolbar), app->play);
    gtk_box_append(GTK_BOX(toolbar), button("Step", G_CALLBACK(on_step), app));
    gtk_box_append(GTK_BOX(toolbar), button("Random", G_CALLBACK(on_random), app));
    gtk_box_append(GTK_BOX(toolbar), button("Glider gun", G_CALLBACK(on_gun), app));
    gtk_box_append(GTK_BOX(toolbar), button("Clear", G_CALLBACK(on_clear), app));
    gtk_box_append(GTK_BOX(toolbar), gtk_label_new("Speed"));
    GtkWidget* speed = gtk_scale_new_with_range(GTK_ORIENTATION_HORIZONTAL, 1, 500, 1);
    gtk_range_set_value(GTK_RANGE(speed), 60);
    gtk_scale_set_value_pos(GTK_SCALE(speed), GTK_POS_RIGHT);
    gtk_widget_set_hexpand(speed, TRUE);
    g_signal_connect(speed, "value-changed", G_CALLBACK(on_speed), app);
    gtk_box_append(GTK_BOX(toolbar), speed);

    app->area = gtk_drawing_area_new();
    gtk_widget_set_hexpand(app->area, TRUE);
    gtk_widget_set_vexpand(app->area, TRUE);
    gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(app->area), draw_board, app, nullptr);
    GtkGesture* drag = gtk_gesture_drag_new();
    g_signal_connect(drag, "drag-begin", G_CALLBACK(on_drag_begin), app);
    g_signal_connect(drag, "drag-update", G_CALLBACK(on_drag_update), app);
    gtk_widget_add_controller(app->area, GTK_EVENT_CONTROLLER(drag));
    gtk_widget_add_tick_callback(app->area, on_tick, app, nullptr);

    app->status = gtk_label_new("");
    gtk_label_set_xalign(GTK_LABEL(app->status), 0);
    gtk_widget_set_margin_start(app->status, 8);
    gtk_widget_set_margin_bottom(app->status, 6);

    GtkWidget* column = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    gtk_box_append(GTK_BOX(column), toolbar);
    gtk_box_append(GTK_BOX(column), app->area);
    gtk_box_append(GTK_BOX(column), app->status);
    gtk_window_set_child(GTK_WINDOW(app->window), column);

    GtkEventController* keys = gtk_event_controller_key_new();
    g_signal_connect(keys, "key-pressed", G_CALLBACK(on_key), app);
    gtk_widget_add_controller(app->window, keys);

    app->simulation.set_speed(60);
    app->simulation.set_running(true);
    if (app->smoke_seconds > 0) {
        g_timeout_add(static_cast<guint>(app->smoke_seconds * 1000), finish_smoke, app);
    }
    gtk_window_present(GTK_WINDOW(app->window));
}

// No window at all: runs the threaded simulation flat out and writes a PNG.
int headless(long generations, const char* png)
{
    life::Simulation simulation(board_width, board_height);
    load_start(simulation, 1);
    simulation.set_speed(100000);
    const auto start = std::chrono::steady_clock::now();
    simulation.set_running(true);
    while (simulation.generation() < static_cast<std::uint64_t>(generations)) {
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    simulation.set_running(false);
    const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();

    life::Grid grid(board_width, board_height);
    simulation.read(grid);
    std::printf("headless: %llu generations in %.3f s (%.0f gen/s), population %d, %d OpenMP threads\n",
        static_cast<unsigned long long>(simulation.generation()), seconds,
        static_cast<double>(simulation.generation()) / seconds, grid.population(), life::threads());
    if (png != nullptr) {
        const bool ok = life::view::save_png(grid, board_width * 5, board_height * 5, png);
        std::printf("%s %s\n", ok ? "saved" : "could not save", png);
        return ok ? 0 : 1;
    }
    return 0;
}

}

int main(int argc, char* argv[])
{
    App* app = nullptr;
    double smoke = 0;
    const char* screenshot = nullptr;
    long headless_generations = -1;
    const char* png = nullptr;
    for (int i = 1; i < argc; ++i) {
        const bool has_value = i + 1 < argc;
        if (std::strcmp(argv[i], "--smoke") == 0 && has_value) {
            if (!parse_number(argv[++i], smoke)) {
                std::fprintf(stderr, "--smoke needs a number of seconds\n");
                return 2;
            }
        } else if (std::strcmp(argv[i], "--screenshot") == 0 && has_value) {
            screenshot = argv[++i];
        } else if (std::strcmp(argv[i], "--headless") == 0 && has_value) {
            if (!parse_number(argv[++i], headless_generations)) {
                std::fprintf(stderr, "--headless needs a number of generations\n");
                return 2;
            }
        } else if (std::strcmp(argv[i], "--png") == 0 && has_value) {
            png = argv[++i];
        } else {
            std::fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    if (headless_generations >= 0) {
        return headless(headless_generations, png);
    }

    app = new App;
    app->smoke_seconds = smoke;
    app->screenshot = screenshot;
    load_start(app->simulation, fresh_seed());

    GtkApplication* gtk_app = gtk_application_new("org.easypp.Life", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(gtk_app, "activate", G_CALLBACK(activate), app);
    // Our options are already handled; GApplication only sees the program name.
    const int status = g_application_run(G_APPLICATION(gtk_app), 1, argv);
    g_object_unref(gtk_app);
    delete app; // stops and joins the simulation thread
    return status;
}
