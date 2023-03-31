# Copyright 2016 The Kubernetes Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

DBG_MAKEFILE ?=
ifeq ($(DBG_MAKEFILE),1)
    $(warning ***** starting Makefile for goal(s) "$(MAKECMDGOALS)")
    $(warning ***** $(shell date))
else
    # If we're not debugging the Makefile, don't echo recipes.
    MAKEFLAGS += -s
endif

# The binaries to build (just the basenames)
BINS ?= placement-manager

# The platforms we support.  In theory this can be used for Windows platforms,
# too, but they require specific base images, which we do not have.
ALL_PLATFORMS ?= linux/amd64 linux/arm linux/arm64 linux/ppc64le linux/s390x

# The "FROM" part of the Dockerfile.  This should be a manifest-list which
# supports all of the platforms listed in ALL_PLATFORMS.
BASE_IMAGE ?= gcr.io/distroless/static

# Where to push the docker images.
REGISTRY ?= 716469478206.dkr.ecr.ap-northeast-1.amazonaws.com

# This version-strategy uses git tags to set the version string
VERSION ?= $(shell git describe --tags --always --dirty)
#
# This version-strategy uses a manual value to set the version string
#VERSION ?= 1.2.3

# Set this to 1 to build a debugger-friendly binaries.
DBG ?=

###
### These variables should not need tweaking.
###

# We don't need make's built-in rules.
MAKEFLAGS += --no-builtin-rules
# Be pedantic about undefined variables.
MAKEFLAGS += --warn-undefined-variables
.SUFFIXES:

# It's necessary to set this because some environments don't link sh -> bash.
SHELL := /usr/bin/env bash -o errexit -o pipefail -o nounset

HOST_OS ?= $(shell if which go > /dev/null; then go env GOOS; fi)
HOST_ARCH ?= $(shell if which go > /dev/null; then go env GOARCH; fi)
ifeq (,$(HOST_OS))
  ifeq ($(OS),Windows_NT)
    HOST_OS := windows
  else
    HOST_OS := $(shell uname -s | tr A-Z a-z)
  endif
endif
ifeq (,$(HOST_ARCH))
  ifeq ($(shell uname -m),x86_64)
    HOST_ARCH := amd64
  else
    HOST_ARCH := $(shell uname -m)
  endif
endif

# Used internally.  Users should pass GOOS and/or GOARCH.
OS := $(if $(GOOS),$(GOOS),$(HOST_OS))
ARCH := $(if $(GOARCH),$(GOARCH),$(HOST_ARCH))

TAG := $(VERSION)__$(OS)_$(ARCH)

GO_VERSION := 1.20
BUILD_IMAGE := golang:$(GO_VERSION)-alpine

BIN_EXTENSION :=
ifeq ($(OS), windows)
  BIN_EXTENSION := .exe
endif

# This is used in docker buildx commands
BUILDX_NAME := $(shell basename $$(pwd))

# Satisfy --warn-undefined-variables.
GOFLAGS ?=
GOPROXY ?= $(shell if which go > /dev/null; then go env GOPROXY; fi)
HTTP_PROXY ?=
HTTPS_PROXY ?=

# Because we store the module cache locally.
GOFLAGS := $(GOFLAGS) -modcacherw

GO_TEST_FLAGS ?=

# If you want to build all binaries, see the 'all-build' rule.
# If you want to build all containers, see the 'all-container' rule.
# If you want to build AND push all containers, see the 'all-push' rule.
all: # @HELP builds binaries for one platform ($OS/$ARCH)
all: build

# For the following OS/ARCH expansions, we transform OS/ARCH into OS_ARCH
# because make pattern rules don't match with embedded '/' characters.

build-%:
	$(MAKE) build                         \
	    --no-print-directory              \
	    GOOS=$(firstword $(subst _, ,$*)) \
	    GOARCH=$(lastword $(subst _, ,$*))

container-%:
	$(MAKE) container                     \
	    --no-print-directory              \
	    GOOS=$(firstword $(subst _, ,$*)) \
	    GOARCH=$(lastword $(subst _, ,$*))

push-%:
	$(MAKE) push                          \
	    --no-print-directory              \
	    GOOS=$(firstword $(subst _, ,$*)) \
	    GOARCH=$(lastword $(subst _, ,$*))

all-build: # @HELP builds binaries for all platforms
all-build: $(addprefix build-, $(subst /,_, $(ALL_PLATFORMS)))

all-container: # @HELP builds containers for all platforms
all-container: $(addprefix container-, $(subst /,_, $(ALL_PLATFORMS)))

all-push: # @HELP pushes containers for all platforms to the defined registry
all-push: $(addprefix push-, $(subst /,_, $(ALL_PLATFORMS)))

# The following structure defeats Go's (intentional) behavior to always touch
# result files, even if they have not changed.  This will still run `go` but
# will not trigger further work if nothing has actually changed.
OUTBINS = $(foreach bin,$(BINS),bin/$(OS)_$(ARCH)/$(bin)$(BIN_EXTENSION))

build: $(OUTBINS)
	echo

# Directories that we need created to build/test.
BUILD_DIRS := bin/$(OS)_$(ARCH)                   \
              bin/tool                            \
              .go/bin/$(OS)_$(ARCH)               \
              .go/bin/$(OS)_$(ARCH)/$(OS)_$(ARCH) \
              .go/cache

# Each outbin target is just a facade for the respective stampfile target.
# This `eval` establishes the dependencies for each.
$(foreach outbin,$(OUTBINS),$(eval  \
    $(outbin): .go/$(outbin).stamp  \
))
# This is the target definition for all outbins.
$(OUTBINS):
	true

# Each stampfile target can reference an $(OUTBIN) variable.
$(foreach outbin,$(OUTBINS),$(eval $(strip   \
    .go/$(outbin).stamp: OUTBIN = $(outbin)  \
)))
# This is the target definition for all stampfiles.
# This will build the binary under ./.go and update the real binary iff needed.
STAMPS = $(foreach outbin,$(OUTBINS),.go/$(outbin).stamp)
.PHONY: $(STAMPS)
$(STAMPS): go-build
	echo -ne "binary: $(OUTBIN)  "
	if ! cmp -s .go/$(OUTBIN) $(OUTBIN); then  \
	    mv .go/$(OUTBIN) $(OUTBIN);            \
	    date >$@;                              \
	    echo;                                  \
	else                                       \
	    echo "(cached)";                       \
	fi

# This runs the actual `go build` which updates all binaries.
go-build: | $(BUILD_DIRS)
	echo "# building for $(OS)/$(ARCH)"
	docker run                                                  \
	    -i                                                      \
	    --rm                                                    \
	    -u $$(id -u):$$(id -g)                                  \
	    -v $$(pwd):/src                                         \
	    -w /src                                                 \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin                \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin/$(OS)_$(ARCH)  \
	    -v $$(pwd)/.go/cache:/.cache                            \
	    --env GOCACHE="/.cache/gocache"                         \
	    --env GOMODCACHE="/.cache/gomodcache"                   \
	    --env ARCH="$(ARCH)"                                    \
	    --env OS="$(OS)"                                        \
	    --env VERSION="$(VERSION)"                              \
	    --env DEBUG="$(DBG)"                                    \
	    --env GOFLAGS="$(GOFLAGS)"                              \
	    --env GOPROXY="$(GOPROXY)"                              \
	    --env HTTP_PROXY="$(HTTP_PROXY)"                        \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"                      \
	    $(BUILD_IMAGE)                                          \
	    ./build/build.sh ./...

# Example: make shell CMD="-c 'date > datefile'"
shell: # @HELP launches a shell in the containerized build environment
shell: | $(BUILD_DIRS)
	echo "# launching a shell in the containerized build environment"
	docker run                                                  \
	    -ti                                                     \
	    --rm                                                    \
	    -u $$(id -u):$$(id -g)                                  \
	    -v $$(pwd):/src                                         \
	    -w /src                                                 \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin                \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin/$(OS)_$(ARCH)  \
	    -v $$(pwd)/.go/cache:/.cache                            \
	    --env GOCACHE="/.cache/gocache"                         \
	    --env GOMODCACHE="/.cache/gomodcache"                   \
	    --env ARCH="$(ARCH)"                                    \
	    --env OS="$(OS)"                                        \
	    --env VERSION="$(VERSION)"                              \
	    --env DEBUG="$(DBG)"                                    \
	    --env GOFLAGS="$(GOFLAGS)"                              \
	    --env GOPROXY="$(GOPROXY)"                              \
	    --env HTTP_PROXY="$(HTTP_PROXY)"                        \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"                      \
	    $(BUILD_IMAGE)                                          \
	    /bin/sh $(CMD)

LICENSES = .licenses

$(LICENSES): | $(BUILD_DIRS)
	# Don't assume that `go` is available locally.
	docker run                                 \
	    -i                                     \
	    --rm                                   \
	    -u $$(id -u):$$(id -g)                 \
	    -v $$(pwd)/tool:/src                   \
	    -w /src                                \
	    -v $$(pwd)/bin/tool:/go/bin            \
	    -v $$(pwd)/.go/cache:/.cache           \
	    --env GOCACHE="/.cache/gocache"        \
	    --env GOMODCACHE="/.cache/gomodcache"  \
	    --env CGO_ENABLED=0                    \
	    --env GOPROXY="$(GOPROXY)"             \
	    --env HTTP_PROXY="$(HTTP_PROXY)"       \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"     \
	    $(BUILD_IMAGE)                         \
	    go install github.com/google/go-licenses
	# The tool runs in a container because it execs `go`, which doesn't
	# play nicely with CI.  The tool also wants its output dir to not
	# exist, so we can't just volume mount $(LICENSES).
	rm -rf $(LICENSES).tmp
	mkdir $(LICENSES).tmp
	docker run                                 \
	    -i                                     \
	    --rm                                   \
	    -u $$(id -u):$$(id -g)                 \
	    -v $$(pwd)/$(LICENSES).tmp:/output     \
	    -v $$(pwd):/src                        \
	    -w /src                                \
	    -v $$(pwd)/bin/tool:/go/bin            \
	    -v $$(pwd)/.go/cache:/.cache           \
	    --env GOCACHE="/.cache/gocache"        \
	    --env GOMODCACHE="/.cache/gomodcache"  \
	    --env HTTP_PROXY="$(HTTP_PROXY)"       \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"     \
	    $(BUILD_IMAGE)                         \
	    go-licenses save ./... --save_path=/output/licenses
	rm -rf $(LICENSES)
	mv $(LICENSES).tmp/licenses $(LICENSES)
	rmdir $(LICENSES).tmp
	find $(LICENSES) -type d | xargs chmod 0755
	find $(LICENSES) -type f | xargs chmod 0644

CONTAINER_DOTFILES = $(foreach bin,$(BINS),.container-$(subst /,_,$(REGISTRY)/$(bin))-$(TAG))

# We print the container names here, rather than in CONTAINER_DOTFILES so
# they are always at the end of the output.
container containers: # @HELP builds containers for one platform ($OS/$ARCH)
container containers: $(CONTAINER_DOTFILES)
	for bin in $(BINS); do                           \
	    echo "container: $(REGISTRY)/$$bin:$(TAG)";  \
	done
	echo

# Each container-dotfile target can reference a $(BIN) variable.
# This is done in 2 steps to enable target-specific variables.
$(foreach bin,$(BINS),$(eval $(strip                                 \
    .container-$(subst /,_,$(REGISTRY)/$(bin))-$(TAG): BIN = $(bin)  \
)))
$(foreach bin,$(BINS),$(eval                                         \
    .container-$(subst /,_,$(REGISTRY)/$(bin))-$(TAG): bin/$(OS)_$(ARCH)/$(bin)$(BIN_EXTENSION) $(LICENSES) Dockerfile.in  \
))
# This is the target definition for all container-dotfiles.
# These are used to track build state in hidden files.
$(CONTAINER_DOTFILES): .buildx-initialized
	echo
	sed                                            \
	    -e 's|{ARG_BIN}|$(BIN)$(BIN_EXTENSION)|g'  \
	    -e 's|{ARG_ARCH}|$(ARCH)|g'                \
	    -e 's|{ARG_OS}|$(OS)|g'                    \
	    -e 's|{ARG_FROM}|$(BASE_IMAGE)|g'          \
	    Dockerfile.in > .dockerfile-$(BIN)-$(OS)_$(ARCH)
	if builtin command -v md5 > /dev/null; then                           \
	    HASH_LICENSES=$$(find $(LICENSES) -type f | xargs md5 | md5);     \
	    HASH_BINARY=$$(md5 -q bin/$(OS)_$(ARCH)/$(BIN)$(BIN_EXTENSION));  \
	else                                                                  \
	    HASH_LICENSES=$$(find $(LICENSES) -type f                         \
	        | xargs md5sum | md5sum | cut -f1 -d' ');                     \
	    HASH_BINARY=$$(md5sum bin/$(OS)_$(ARCH)/$(BIN)$(BIN_EXTENSION)    \
	        | cut -f1 -d' ');                                             \
	fi;                                                                   \
	FORCE=0;                                                              \
	docker buildx build                                                   \
	    --builder "$(BUILDX_NAME)"                                        \
	    --build-arg FORCE_REBUILD="$$FORCE"                               \
	    --build-arg HASH_LICENSES="$$HASH_LICENSES"                       \
	    --build-arg HASH_BINARY="$$HASH_BINARY"                           \
	    --progress=plain                                                  \
	    --load                                                            \
	    --platform "$(OS)/$(ARCH)"                                        \
	    --build-arg HTTP_PROXY="$(HTTP_PROXY)"                            \
	    --build-arg HTTPS_PROXY="$(HTTPS_PROXY)"                          \
	    -t $(REGISTRY)/$(BIN):$(TAG)                                      \
	    -f .dockerfile-$(BIN)-$(OS)_$(ARCH)                               \
	    .
	docker images -q $(REGISTRY)/$(BIN):$(TAG) > $@
	echo

push: # @HELP pushes the container for one platform ($OS/$ARCH) to the defined registry
push: container
	for bin in $(BINS); do                     \
	    docker push $(REGISTRY)/$$bin:$(TAG);  \
	done
	echo

# This depends on github.com/estesp/manifest-tool.
manifest-list: # @HELP builds a manifest list of containers for all platforms
manifest-list: all-push
	# Don't assume that `go` is available locally.
	docker run                                 \
	    -i                                     \
	    --rm                                   \
	    -u $$(id -u):$$(id -g)                 \
	    -v $$(pwd)/tool:/src                   \
	    -w /src                                \
	    -v $$(pwd)/bin/tool:/go/bin            \
	    -v $$(pwd)/.go/cache:/.cache           \
	    --env GOCACHE="/.cache/gocache"        \
	    --env GOMODCACHE="/.cache/gomodcache"  \
	    --env GOARCH="$(HOST_ARCH)"            \
	    --env GOOS="$(HOST_OS)"                \
	    --env CGO_ENABLED=0                    \
	    --env GOPROXY="$(GOPROXY)"             \
	    --env HTTP_PROXY="$(HTTP_PROXY)"       \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"     \
	    $(BUILD_IMAGE)                         \
	    go install github.com/estesp/manifest-tool/v2/cmd/manifest-tool
	for bin in $(BINS); do                                        \
	    platforms=$$(echo $(ALL_PLATFORMS) | sed 's/ /,/g');      \
	    if [ -e bin/tool/manifest-tool ]; then                    \
	        bin/tool/manifest-tool                                \
	            push from-args                                    \
	            --platforms "$$platforms"                         \
	            --template $(REGISTRY)/$$bin:$(VERSION)__OS_ARCH  \
	            --target $(REGISTRY)/$$bin:$(VERSION);            \
	    else                                                      \
	        bin/tool/$(HOST_OS)_$(HOST_ARCH)/manifest-tool        \
	           push from-args                                     \
	           --platforms "$$platforms"                          \
	           --template $(REGISTRY)/$$bin:$(VERSION)__OS_ARCH   \
	           --target $(REGISTRY)/$$bin:$(VERSION);             \
	    fi                                                        \
	done

version: # @HELP outputs the version string
version:
	echo $(VERSION)

test: # @HELP runs tests, as defined in ./build/test.sh
test: | $(BUILD_DIRS)
	docker run                                                  \
	    -i                                                      \
	    --rm                                                    \
	    -u $$(id -u):$$(id -g)                                  \
	    -v $$(pwd):/src                                         \
	    -w /src                                                 \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin                \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin/$(OS)_$(ARCH)  \
	    -v $$(pwd)/.go/cache:/.cache                            \
	    --env GOCACHE="/.cache/gocache"                         \
	    --env GOMODCACHE="/.cache/gomodcache"                   \
	    --env ARCH="$(ARCH)"                                    \
	    --env OS="$(OS)"                                        \
	    --env VERSION="$(VERSION)"                              \
	    --env DEBUG="$(DBG)"                                    \
	    --env GOFLAGS="$(GOFLAGS)"                              \
	    --env GOPROXY="$(GOPROXY)"                              \
	    --env HTTP_PROXY="$(HTTP_PROXY)"                        \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"                      \
	    --env GO_TEST_FLAGS="$(GO_TEST_FLAGS)"                  \
	    $(BUILD_IMAGE)                                          \
	    ./build/test.sh ./...

lint: # @HELP runs golangci-lint
lint: | $(BUILD_DIRS)
	docker run                                                  \
	    -i                                                      \
	    --rm                                                    \
	    -u $$(id -u):$$(id -g)                                  \
	    -v $$(pwd):/src                                         \
	    -w /src                                                 \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin                \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin/$(OS)_$(ARCH)  \
	    -v $$(pwd)/.go/cache:/.cache                            \
	    --env GOCACHE="/.cache/gocache"                         \
	    --env GOMODCACHE="/.cache/gomodcache"                   \
	    --env ARCH="$(ARCH)"                                    \
	    --env OS="$(OS)"                                        \
	    --env VERSION="$(VERSION)"                              \
	    --env DEBUG="$(DBG)"                                    \
	    --env GOFLAGS="$(GOFLAGS)"                              \
	    --env GOPROXY="$(GOPROXY)"                              \
	    --env HTTP_PROXY="$(HTTP_PROXY)"                        \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"                      \
	    $(BUILD_IMAGE)                                          \
	    ./build/lint.sh ./...

fmt: # @HELP runs gofmt with -w flag
fmt: | $(BUILD_DIRS)
	docker run                                                  \
	    -i                                                      \
	    --rm                                                    \
	    -u $$(id -u):$$(id -g)                                  \
	    -v $$(pwd):/src                                         \
	    -w /src                                                 \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin                \
	    -v $$(pwd)/.go/bin/$(OS)_$(ARCH):/go/bin/$(OS)_$(ARCH)  \
	    -v $$(pwd)/.go/cache:/.cache                            \
	    --env GOCACHE="/.cache/gocache"                         \
	    --env GOMODCACHE="/.cache/gomodcache"                   \
	    --env ARCH="$(ARCH)"                                    \
	    --env OS="$(OS)"                                        \
	    --env VERSION="$(VERSION)"                              \
	    --env DEBUG="$(DBG)"                                    \
	    --env GOFLAGS="$(GOFLAGS)"                              \
	    --env GOPROXY="$(GOPROXY)"                              \
	    --env HTTP_PROXY="$(HTTP_PROXY)"                        \
	    --env HTTPS_PROXY="$(HTTPS_PROXY)"                      \
	    $(BUILD_IMAGE)                                          \
	    ./build/fmt.sh

$(BUILD_DIRS):
	mkdir -p $@

clean: # @HELP removes built binaries and temporary files
clean: container-clean bin-clean

container-clean:
	rm -rf .container-* .dockerfile-* .push-* .buildx-initialized $(LICENSES)

bin-clean:
	test -d .go && chmod -R u+w .go || true
	rm -rf .go bin

help: # @HELP prints this message
help:
	echo "VARIABLES:"
	echo "  BINS = $(BINS)"
	echo "  OS = $(OS)"
	echo "  ARCH = $(ARCH)"
	echo "  DBG = $(DBG)"
	echo "  GOFLAGS = $(GOFLAGS)"
	echo "  GOPROXY = $(GOPROXY)"
	echo "  REGISTRY = $(REGISTRY)"
	echo
	echo "TARGETS:"
	grep -E '^.*: *# *@HELP' $(MAKEFILE_LIST)     \
	    | awk '                                   \
	        BEGIN {FS = ": *# *@HELP"};           \
	        { printf "  %-30s %s\n", $$1, $$2 };  \
	    '

# Help set up multi-arch build tools.  This assumes you have the tools
# installed.  If you already have a buildx builder available, you don't need
# this.  See https://medium.com/@artur.klauser/building-multi-architecture-docker-images-with-buildx-27d80f7e2408
# for great context.
.buildx-initialized:
	docker buildx create --name "$(BUILDX_NAME)" --node "$(BUILDX_NAME)-0" >/dev/null
	if ! docker version | grep -iq "Docker Desktop"; then                                 \
	  docker run --rm --privileged multiarch/qemu-user-static --reset -p yes >/dev/null;  \
	fi
	date > $@
