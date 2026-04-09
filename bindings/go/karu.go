// SPDX-License-Identifier: MIT

// Package karu provides Go bindings for the Karu policy engine.
//
// Karu is an embeddable policy engine focusing on structural pattern matching
// over arbitrary JSON data. This package wraps the native C FFI via cgo.
//
// The caller must provide a pre-built libkaru static library (libkaru.a) and
// the generated karu.h header. Set CGO_LDFLAGS and CGO_CFLAGS as needed:
//
//	export CGO_LDFLAGS="-L/path/to/lib -lkaru -ldl -lm -lpthread"
//	export CGO_CFLAGS="-I/path/to/include"
package karu

/*
#cgo LDFLAGS: -lkaru -ldl -lm -lpthread
#include <karu.h>
#include <stdlib.h>
*/
import "C"
import (
	"errors"
	"runtime"
	"unsafe"
)

// Effect represents a policy evaluation result.
type Effect int

const (
	// Allow means the policy permits the request.
	Allow Effect = 1
	// Deny means the policy denies the request.
	Deny Effect = 0
)

func (e Effect) String() string {
	switch e {
	case Allow:
		return "ALLOW"
	case Deny:
		return "DENY"
	default:
		return "UNKNOWN"
	}
}

// Policy is a compiled Karu policy. It is safe for concurrent use.
type Policy struct {
	ptr *C.KaruPolicy
}

// Compile parses and compiles a Karu policy from source.
func Compile(source string) (*Policy, error) {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))

	ptr := C.karu_compile((*C.uchar)(unsafe.Pointer(cs)), C.ulong(len(source)))
	if ptr == nil {
		return nil, errors.New("karu: failed to compile policy")
	}

	p := &Policy{ptr: ptr}
	runtime.SetFinalizer(p, func(p *Policy) {
		p.Close()
	})
	return p, nil
}

// Evaluate evaluates the compiled policy against a JSON input string.
func (p *Policy) Evaluate(jsonInput string) (Effect, error) {
	if p.ptr == nil {
		return Deny, errors.New("karu: policy is closed")
	}

	ci := C.CString(jsonInput)
	defer C.free(unsafe.Pointer(ci))

	result := C.karu_evaluate(
		p.ptr,
		(*C.uchar)(unsafe.Pointer(ci)),
		C.ulong(len(jsonInput)),
	)

	switch result {
	case 1:
		return Allow, nil
	case 0:
		return Deny, nil
	default:
		return Deny, errors.New("karu: evaluation error")
	}
}

// Close releases the native resources associated with this policy.
// It is safe to call Close multiple times.
func (p *Policy) Close() {
	if p.ptr != nil {
		C.karu_policy_free(p.ptr)
		p.ptr = nil
	}
}

// EvalOnce compiles and evaluates a policy in a single call.
// This is a convenience function for one-shot evaluations.
func EvalOnce(source, jsonInput string) (Effect, error) {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	ci := C.CString(jsonInput)
	defer C.free(unsafe.Pointer(ci))

	result := C.karu_eval_once(
		(*C.uchar)(unsafe.Pointer(cs)),
		C.ulong(len(source)),
		(*C.uchar)(unsafe.Pointer(ci)),
		C.ulong(len(jsonInput)),
	)

	switch result {
	case 1:
		return Allow, nil
	case 0:
		return Deny, nil
	default:
		return Deny, errors.New("karu: evaluation error")
	}
}
