# Alcro

[![Build Status](https://travis-ci.com/Srinivasa314/alcro.svg?branch=master)](https://travis-ci.com/Srinivasa314/alcro)
[![Crates.io](https://img.shields.io/crates/v/alcro)](https://crates.io/crates/alcro)

A small library to build desktop apps using Rust and modern web technologies. It uses Chrom(e/ium) or similar browsers like MS Edge (new) for UI. It does not bundle Chrome but instead communicates with the existing Chrome installation.
Thus Rust functions can be called from the UI and JavaScript can be called from Rust.

#### Name
Alcro works similarily to the go library [lorca](https://github.com/zserge/lorca) so the name alcro is an anagram of lorca. However it uses pipes unlike lorca which uses a websocket. 

## Documentation
[docs.rs](https://docs.rs/alcro/0.5.4/alcro/)

## Examples
[https://github.com/Srinivasa314/alcro/tree/master/examples](https://github.com/Srinivasa314/alcro/tree/master/examples)

## Features
* Small applications
* Use web technologies for UI and use safe and fast rust code.
* Can control and get position, size and state of window
* Expose rust functions to Javascript
* Call any JS code from rust
* Exposed rust functions are executed in a new thread and can be called asynchronously
* Load HTML from url, local file or even embedded files
* JS console messages and exceptions are printed for easier debugging
* Can run in headless mode
* Supports running many windows (Currently each window should have a different user data directory)

## Limitations
* Requires Chrom(e/ium) to be installed
* Native systray, etc. needs third party crates

## How it works
Alcro uses the Chrome DevTools protocol and communicates with it via a pipe.
