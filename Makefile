clean:
	rm -rf ./release/rsocket-cli/usr

build: clean
	cargo build --release

release: build
	mkdir -p ./release/rsocket-cli/usr/local/bin/
	cp ./target/release/rsocket-cli ./release/rsocket-cli/usr/local/bin/
	dpkg-deb --build ./release/rsocket-cli
	echo "Release available at ./release/rsocket-cli.deb"
