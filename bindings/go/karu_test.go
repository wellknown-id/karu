// SPDX-License-Identifier: MIT

package karu

import (
	"testing"
)

func TestCompileAndEvaluate(t *testing.T) {
	policy, err := Compile(`allow access if role == "admin";`)
	if err != nil {
		t.Fatalf("Compile failed: %v", err)
	}
	defer policy.Close()

	effect, err := policy.Evaluate(`{"role": "admin"}`)
	if err != nil {
		t.Fatalf("Evaluate failed: %v", err)
	}
	if effect != Allow {
		t.Errorf("expected Allow, got %v", effect)
	}

	effect, err = policy.Evaluate(`{"role": "user"}`)
	if err != nil {
		t.Fatalf("Evaluate failed: %v", err)
	}
	if effect != Deny {
		t.Errorf("expected Deny, got %v", effect)
	}
}

func TestEvalOnce(t *testing.T) {
	effect, err := EvalOnce(
		`allow access if value > 10;`,
		`{"value": 15}`,
	)
	if err != nil {
		t.Fatalf("EvalOnce failed: %v", err)
	}
	if effect != Allow {
		t.Errorf("expected Allow, got %v", effect)
	}
}

func TestCompileError(t *testing.T) {
	_, err := Compile("allow if if if {{{")
	if err == nil {
		t.Error("expected error for invalid policy")
	}
}

func TestEffectString(t *testing.T) {
	if Allow.String() != "ALLOW" {
		t.Errorf("expected ALLOW, got %s", Allow.String())
	}
	if Deny.String() != "DENY" {
		t.Errorf("expected DENY, got %s", Deny.String())
	}
}
