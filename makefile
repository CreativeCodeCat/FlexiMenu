# FlexiMenu - dynamic menu
# See LICENSE file for copyright and license details.

include config.mk

ifeq ($(XINERAMA),true)
	XINERAMA_FLAGS = --all-features # idk if there will ever be a workaround
endif

ifeq ($(CC),)
	CC = cc
endif

export CARGOFLAGS
export RUSTFLAGS
export PLUGINS
export VERSION
export XINERAMA
export depcheck
export CC

all:	options fleximenu stest

options:
	@echo "fleximenu ($(VERSION)) build options:"
	@echo "CC         = $(CC)"
	@echo "RUSTFLAGS  = $(RUSTFLAGS)"
	@echo "PLUGINS    = $(PLUGINS)"

config:	scaffold
	cd src && cargo run --release -p config --bin config
	$(MAKE) m4

fleximenu:	config
	cd src && cargo run --release -p headers
	cd src && cargo build -p fleximenu-build --release $(XINERAMA_FLAGS) $(CARGOFLAGS)
	cp src/target/release/fleximenu target/

man:	config
	man target/fleximenu.1

test:	all
	cd src && cargo test
	seq 1 100 | target/fleximenu $(ARGS)

debug:	config
	cd src && cargo build -p fleximenu-build $(XINERAMA_FLAGS) $(CARGOFLAGS)
	cp src/target/debug/fleximenu target
	seq 1 100 | target/fleximenu $(ARGS)

plugins:
	cd src && cargo run --release -p config --bin list-plugins

stest:
	cd src && cargo build -p stest --release $(XINERAMA_FLAGS) $(CARGOFLAGS)
	cp src/target/release/stest target/
	cp src/man/src/stest.1 target/

scaffold:
	mkdir -p target
	mkdir -p target/build
	touch target/build/deps.toml
	$(MAKE) m4 # second round will finish deps

m4:
	m4 src/build/CargoSource.toml > target/build/Cargo.toml
	test -f src/build/Cargo.toml || cp target/build/Cargo.toml src/build/Cargo.toml
	cmp -s -- target/build/Cargo.toml src/build/Cargo.toml || cp target/build/Cargo.toml src/build/Cargo.toml

clean:	scaffold
	cd src && cargo clean -p config -p fleximenu-build -p headers
	rm -rf src/target
	rm -f vgcore* massif* src/build/Cargo.toml
	rm -rf target
	rm -rf fleximenu-* # distribution files
	rm -f src/Cargo.lock

fmt:	config
	cd src && cargo fmt

version:
	@echo -n "${VERSION}"

dist:	
	mkdir -p fleximenu-$(VERSION)
	cp -r LICENSE README.md makefile config.mk src fleximenu-$(VERSION)
	tar -cf fleximenu-$(VERSION).tar fleximenu-$(VERSION)
	gzip fleximenu-$(VERSION).tar
	rm -rf fleximenu-$(VERSION)

# may need sudo
install:	all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f target/fleximenu src/sh/fleximenu_path src/sh/fleximenu_run target/stest $(DESTDIR)$(PREFIX)/bin/
	chmod 755 $(DESTDIR)$(PREFIX)/bin/fleximenu
	chmod 755 $(DESTDIR)$(PREFIX)/bin/fleximenu_path
	chmod 755 $(DESTDIR)$(PREFIX)/bin/fleximenu_run
	chmod 755 $(DESTDIR)$(PREFIX)/bin/stest
	mkdir -p $(DESTDIR)$(MANPREFIX)/man1
	cp target/fleximenu.1 $(DESTDIR)$(MANPREFIX)/man1/fleximenu.1
	sed "s/VERSION/$(VERSION)/g" < target/stest.1 > $(DESTDIR)$(MANPREFIX)/man1/stest.1
	chmod 644 $(DESTDIR)$(MANPREFIX)/man1/fleximenu.1
	chmod 644 $(DESTDIR)$(MANPREFIX)/man1/stest.1

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/fleximenu\
		$(DESTDIR)$(PREFIX)/bin/fleximenu_path\
		$(DESTDIR)$(PREFIX)/bin/fleximenu_run\
		$(DESTDIR)$(PREFIX)/bin/stest\
		$(DESTDIR)$(MANPREFIX)/man1/fleximenu.1\
		$(DESTDIR)$(MANPREFIX)/man1/stest.1
