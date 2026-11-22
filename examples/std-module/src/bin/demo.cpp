import std;

int main()
{
    const std::vector values{10, 20, 12};
    std::cout << std::accumulate(values.begin(), values.end(), 0) << '\n';
}
