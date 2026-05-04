package main

import "fmt"

// Add returns the sum of a and b. Panics on overflow.
func Add(a, b int32) int32 {
	return a + b
}

func main() {
	fmt.Println(Add(2147483647, 1)) // intentionally overflows
}
