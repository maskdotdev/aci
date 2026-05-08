#include <vector>

namespace demo {
class Widget {
public:
    int size() const {
        return 1;
    }
};

int make_widget() {
    Widget widget;
    return widget.size();
}
}
