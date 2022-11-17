
Reading RDM3600 from a raspberry pi using rppal.

## Build

### Static Cross-Compile (`cross`)
```bash
# Use different target folder to ensure no conflict with non-cross-compile build
env CARGO_TARGET_DIR=target-cross cross build --target=arm-unknown-linux-musleabihf
```

## Run

```bash
rsync -hh -P ./target-cross/arm-unknown-linux-musleabihf/debug/block-rppal <pi-ip>:/tmp
ssh <pi-ip> "/tmp/block-rppal"
```