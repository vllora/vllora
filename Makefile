ONNX_VERSION:=1.17.3
PROFILE:=release

X86_CONTAINER_TARGET=x86_64-unknown-linux-gnu
ARM_CONTAINER_TARGET=aarch64-unknown-linux-gnu

CONTAINER_GLIBC=2.31

ROOT_DIR=$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
DEPDIR=${ROOT_DIR}/target/deps
TMPDIR=${ROOT_DIR}/target/tmp
SYSTEM_TARGET=$(shell rustc -vV | sed -n 's|host: ||p')

ifeq (${PROFILE},dev)
	PROFILE_DIR=debug
else
	PROFILE_DIR=release
endif

# Function to get target directory for a specific target
target_dir = ${ROOT_DIR}/target/$(1)/${PROFILE_DIR}

# Local build targets
build_local: udf_local
	cp $(call target_dir,${X86_CONTAINER_TARGET})/langdb_udf \
	docker/clickhouse/user_scripts/langdb_udf

udf_local: ${TMPDIR}
	cargo zigbuild --profile ${PROFILE} \
		--target ${X86_CONTAINER_TARGET} \
		--bin langdb_udf

gateway_local: ${TMPDIR}
	cargo zigbuild --profile ${PROFILE} \
		--target ${X86_CONTAINER_TARGET} \
		--bin ai-gateway

# Multi-architecture build targets
build_all: build_udfs build_gateways

build_udfs: ${TMPDIR}
	cargo zigbuild --profile ${PROFILE} \
		--target ${X86_CONTAINER_TARGET} \
		--target ${ARM_CONTAINER_TARGET} \
		--bin langdb_udf

build_gateways: ${TMPDIR}
	cargo zigbuild --profile ${PROFILE} \
		--target ${X86_CONTAINER_TARGET} \
		--target ${ARM_CONTAINER_TARGET} \
		--bin ai-gateway

${TMPDIR}:
	mkdir -p ${TMPDIR}
	mkdir -p ${DEPDIR}

FORCE: ;

