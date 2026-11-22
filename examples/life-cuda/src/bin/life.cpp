#include "life/life.hpp"

#include <gtk/gtk.h>

#include <cstdio>
#include <string_view>

namespace {

struct App {
    life::CudaLife cuda;
    life::Board board = life::seeded_board();
    GtkApplication* application = nullptr;
    GtkWidget* area = nullptr;
    GtkWidget* status = nullptr;
    GtkWidget* play = nullptr;
    unsigned generation = 0;
    guint timer = 0;
    bool playing = true;
    bool smoke = false;
    bool failed = false;
};

void refresh(App& app)
{
    char label[100];
    std::snprintf(label, sizeof(label), "Generation %u  ·  %d live cells", app.generation, life::population(app.board));
    gtk_label_set_text(GTK_LABEL(app.status), label);
    gtk_button_set_label(GTK_BUTTON(app.play), app.playing ? "Pause" : "Play");
    gtk_widget_queue_draw(app.area);
}

void step(App& app)
{
    if (const char* error = app.cuda.step(app.board)) {
        std::fprintf(stderr, "CUDA: %s\n", error);
        gtk_label_set_text(GTK_LABEL(app.status), error);
        app.playing = false;
        app.failed = true;
        if (app.smoke) {
            g_application_quit(G_APPLICATION(app.application));
        }
        return;
    }
    ++app.generation;
    refresh(app);
}

void draw(GtkDrawingArea*, cairo_t* cr, int width, int height, gpointer data)
{
    const auto& board = static_cast<App*>(data)->board;
    const double cell_width = static_cast<double>(width) / life::width;
    const double cell_height = static_cast<double>(height) / life::height;

    cairo_set_source_rgb(cr, 0.05, 0.10, 0.12);
    cairo_paint(cr);
    cairo_set_source_rgb(cr, 0.52, 0.91, 0.69);
    for (int y = 0; y < life::height; ++y) {
        for (int x = 0; x < life::width; ++x) {
            if (board[y * life::width + x]) {
                cairo_rectangle(cr, x * cell_width, y * cell_height, cell_width * 0.9, cell_height * 0.9);
            }
        }
    }
    cairo_fill(cr);
}

void on_play(GtkButton*, gpointer data)
{
    auto& app = *static_cast<App*>(data);
    app.playing = !app.playing;
    refresh(app);
}

void on_step(GtkButton*, gpointer data)
{
    auto& app = *static_cast<App*>(data);
    app.playing = false;
    step(app);
}

void on_reset(GtkButton*, gpointer data)
{
    auto& app = *static_cast<App*>(data);
    app.board = life::seeded_board();
    app.generation = 0;
    refresh(app);
}

void on_cell_pressed(GtkGestureClick*, int, double x, double y, gpointer data)
{
    auto& app = *static_cast<App*>(data);
    const int width = gtk_widget_get_width(app.area);
    const int height = gtk_widget_get_height(app.area);
    if (width <= 0 || height <= 0) {
        return;
    }
    const int column = static_cast<int>(x * life::width / width);
    const int row = static_cast<int>(y * life::height / height);
    if (column >= 0 && column < life::width && row >= 0 && row < life::height) {
        app.board[row * life::width + column] ^= 1;
        refresh(app);
    }
}

gboolean tick(gpointer data)
{
    auto& app = *static_cast<App*>(data);
    if (app.playing) {
        step(app);
    }
    if (app.smoke && app.generation >= 4) {
        g_application_quit(G_APPLICATION(app.application));
    }
    return G_SOURCE_CONTINUE;
}

void activate(GtkApplication* application, gpointer data)
{
    auto& app = *static_cast<App*>(data);
    auto* window = gtk_application_window_new(application);
    gtk_window_set_title(GTK_WINDOW(window), "Conway's Game of Life");
    gtk_window_set_default_size(GTK_WINDOW(window), 860, 660);

    auto* column = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    auto* toolbar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_margin_start(toolbar, 12);
    gtk_widget_set_margin_end(toolbar, 12);
    gtk_widget_set_margin_top(toolbar, 8);
    gtk_widget_set_margin_bottom(toolbar, 8);
    app.play = gtk_button_new_with_label("Pause");
    auto* advance = gtk_button_new_with_label("Step");
    auto* reset = gtk_button_new_with_label("Reset");
    app.status = gtk_label_new(nullptr);
    g_signal_connect(app.play, "clicked", G_CALLBACK(on_play), &app);
    g_signal_connect(advance, "clicked", G_CALLBACK(on_step), &app);
    g_signal_connect(reset, "clicked", G_CALLBACK(on_reset), &app);
    gtk_box_append(GTK_BOX(toolbar), app.play);
    gtk_box_append(GTK_BOX(toolbar), advance);
    gtk_box_append(GTK_BOX(toolbar), reset);
    gtk_box_append(GTK_BOX(toolbar), app.status);

    app.area = gtk_drawing_area_new();
    gtk_widget_set_vexpand(app.area, TRUE);
    gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(app.area), draw, &app, nullptr);
    auto* click = gtk_gesture_click_new();
    g_signal_connect(click, "pressed", G_CALLBACK(on_cell_pressed), &app);
    gtk_widget_add_controller(app.area, GTK_EVENT_CONTROLLER(click));

    gtk_box_append(GTK_BOX(column), toolbar);
    gtk_box_append(GTK_BOX(column), app.area);
    gtk_window_set_child(GTK_WINDOW(window), column);
    refresh(app);
    gtk_window_present(GTK_WINDOW(window));
    app.timer = g_timeout_add(85, tick, &app);
}

int check(App& app)
{
    auto reference = app.board;
    for (int generation = 1; generation <= 24; ++generation) {
        reference = life::reference_step(reference);
        if (const char* error = app.cuda.step(app.board)) {
            std::fprintf(stderr, "CUDA: %s\n", error);
            return 1;
        }
        if (app.board != reference) {
            std::fprintf(stderr, "Generation %d differs from the OpenMP reference\n", generation);
            return 1;
        }
    }
    std::printf("CUDA and OpenMP matched for 24 generations (%d live cells)\n", life::population(app.board));
    return 0;
}

}

int main(int argc, char** argv)
{
    App app;
    if (const char* error = app.cuda.error()) {
        std::fprintf(stderr, "CUDA: %s\n", error);
        return 1;
    }
    const std::string_view mode = argc > 1 ? argv[1] : "";
    if (mode == "--check") {
        return check(app);
    }
    app.smoke = mode == "--smoke";

    app.application = gtk_application_new("org.easypp.LifeCuda", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app.application, "activate", G_CALLBACK(activate), &app);
    const int status = g_application_run(G_APPLICATION(app.application), app.smoke ? 1 : argc, argv);
    if (app.timer != 0) {
        g_source_remove(app.timer);
    }
    g_object_unref(app.application);
    return app.failed ? 1 : status;
}
