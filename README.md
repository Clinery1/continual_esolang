# About
*Continual* is an experiment in first-class continuations. It explores what happens if continuations
are the main control flow primitive, the program stops when a continuation "returns," and if the
current continuation is always passed as a function's first argument.


# Cool things about the language
## Tail call optimization
Because root continuations are cloned when executed, and that we don't recurse when applying a new
continuation, tail calls in the language are free.

## Implementing native functions is very easy
Due to how continuations are made, I only need to pass the root scope and args to a native function.

## The interpreter
The current interpreter is implemented in a decentralized way. There is no single object that holds
all the state, but instead, each continuation holds its own scopes and state and is dropped when it
is no longer being used. I think this is cool because you can inject different continuation types
that do different things... Whatever that means.


# Code examples
## Hello world
```lisp
(defCont main []
    (println "Hello, world!"))
```

## FizzBuzz
```text
(defCont fizzBuzz [ret count]
    (apply fizzBuzzInner ret count 1))
(defCont fizzBuzzInner [ret count i]
    (set five (eq (rem i 5) 0))
    (set three (eq (rem i 3) 0))

    (if (and five three)
        (println "FizzBuzz")
        (if five
            (println "Fizz")
            (if three
                (println "Buzz")
                (println i))))

    (if (eq i count) (apply ret))
    (apply fizzBuzzInner ret count (add 1 i)))
```

## Fibonacci sequence
```
(defCont fib [ret count]
    (if (eq count 0) (apply ret 0))
    (apply fibInner ret count 0 1))
(defCont fibInner [ret count a b]
    (if (eq count 1) (apply ret b))

    (apply fibInner ret (sub count 1) b (add a b)))
```

## Jumping around with continuations
```
(defCont doAThing [ret other]
    (println "Do a thing")
    (apply ret other))

(defCont doAnotherThing [ret]
    (println "Do another thing")
    (apply ret))

(defCont main []
    (doAThing exit))

(defCont exit [])
```

# Notes
Currently, if you store the first argument after being called like normal, then it will likely cause
a bug just based on how the current interpreter is implemented. The way to fix it is to write a
bytecode interpreter instead of a tree walking interpreter, but this is just an experiment, so I
won't do that.
