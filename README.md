# Alcro
A small library to build desktop apps using rust and modern web technologies. It uses Chrom(e/ium) browser for UI. It does not bundle chrome but instead communicates with the existing chrome installation.
Thus rust functions can be called from the UI and javascript can be called Rust.

#### Name
Alcro works similarily to the go library [lorca](https://github.com/zserge/lorca) so the name alcro is an anagram of lorca.

## Documentation
[docs.rs](https://docs.rs/alcro/0.1.0/alcro/)

## Examples
[https://github.com/Srinivasa314/alcro/tree/master/examples](https://github.com/Srinivasa314/alcro/tree/master/examples)

## Features
* Small applications
* Use web technologies for UI and use safe and fast rust code.
* Can control and get position, size and state of window
* Expose rust functions to Javascript
* Call any JS code from rust
* Exposed functions are asynchronous
* Load HTML from url, local file or even embedded files
* JS console messages and exceptions are printed for easier debugging
* Can run in headless mode
* Supports running many windows

## Limitations
* Requires Chrom(e/ium) to be installed
* Native systray, etc. needs third party crates

## Working
Alcro uses the Chrome DevTools protocol and communicates with it via a pipe.
