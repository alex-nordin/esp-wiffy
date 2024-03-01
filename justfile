set shell := ["nu", "-c"]

alias b := build
alias bc := bacon

build:
	cargo build --release

bacon:
	bacon --all-features
