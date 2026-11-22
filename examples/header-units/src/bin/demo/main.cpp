import "constants.hpp";
import "scale/scale.hpp";
import <array>;

int main()
{
    const std::array values{input};
    return scale(values.front()) == 42 ? 0 : 1;
}
