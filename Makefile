ONNX_VERSION:=1.17.3
PROFILE:=release

CONTAINER_TARGET=x86_64-unknown-linux-gnu
CONTAINER_GLIBC=2.31

ROOT_DIR=$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
DEPDIR=${TARGETDIR}/deps
TMPDIR=${TARGETDIR}/tmp
SYSTEM_TARGET=$(shell rustc -vV | sed -n 's|host: ||p')

ifeq (${PROFILE},dev)
	PROFILE_DIR=debug
else
	PROFILE_DIR=release
endif

TARGETDIR=${ROOT_DIR}/target/${CONTAINER_TARGET}/${PROFILE_DIR}

ifeq (${SYSTEM_TARGET}, ${CONTAINER_TARGET})
	RUN_CMD=cargo-zigbuild run --target ${CONTAINER_TARGET}.${CONTAINER_GLIBC}
else
	RUN_CMD=cargo run
endif

build: ${TARGETDIR}/udf
	cp ${TARGETDIR}/udf docker/clickhouse/user_scripts/langdb_udf

${TARGETDIR}/udf: ${TMPDIR} FORCE
	cargo zigbuild --profile ${PROFILE} --target ${CONTAINER_TARGET}.${CONTAINER_GLIBC} --bin udf


${TMPDIR}:
	mkdir -p ${TMPDIR}

FORCE: ;
