package demo

import "fmt"

type Counter struct {
    value int
}

func Add(left int, right int) int {
    return left + right
}

func (counter *Counter) Inc() {
    counter.value = Add(counter.value, 1)
    fmt.Println(counter.value)
}
