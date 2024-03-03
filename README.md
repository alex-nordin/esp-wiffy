esp-wiffy
===========

Rust project built for the esp32c3 board. Experiment leveraging the 
embassy async runtime running in a no_std, baremetal environment as well as
wifi connectivity. 

Code connects to a local wifi network then asyncronously takes measurements from two connected sensors.
One provides reading on temperature and humidity (dht11), and the other sends a warning if it detects a fire.


## Build Instructions
Use the nix flake in the root of this repo. It sets up a development environment in
your shell containing everything you need to build the project, provided you have a
esp32c3 board connected vie USB. Replace the age encrypted files in the secrets directory
with your own wifi ssid & password etc. and in the enterShell hook in flake.nix, 
change the hardcoded path to the identity file to wherever you have your age keyfile.

If you've done all that you should now be able to build and flash the project with ```cargo run --release```.
