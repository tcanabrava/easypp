import std.compat;

int main()
{
    char buffer[8]{};
    return ::snprintf(buffer, sizeof(buffer), "%d", 42) == 2
            && std::string_view{buffer} == "42"
        ? 0
        : 1;
}
