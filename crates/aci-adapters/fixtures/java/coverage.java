package demo;

import java.util.List;

public class Widget {
    private int size;

    public Widget(int size) {
        this.size = size;
    }

    public int size() {
        return List.of(size).get(0);
    }
}
