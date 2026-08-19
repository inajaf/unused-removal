package config

import "testing"

func TestExpandPercentVars(t *testing.T) {
	t.Setenv("TESTVAR", "C:\\Temp\\Value")

	tests := []struct {
		name string
		in   string
		want string
	}{
		{"percent expanded", `%TESTVAR%\sub`, `C:\Temp\Value\sub`},
		{"dollar untouched", `$Recycle.Bin`, `$Recycle.Bin`},
		{"missing percent kept", `%NO_SUCH_VAR%\x`, `%NO_SUCH_VAR%\x`},
		{"no vars", `C:\Windows\Temp`, `C:\Windows\Temp`},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := expandPercentVars(tc.in)
			if got != tc.want {
				t.Errorf("expandPercentVars(%q) = %q, want %q", tc.in, got, tc.want)
			}
		})
	}
}

func TestExpandEnvVars(t *testing.T) {
	t.Setenv("TEMP_TEST", "D:\\Temp")

	got := expandEnvVars([]string{`%TEMP_TEST%\a`, `$Recycle.Bin`})
	if got[0] != `D:\Temp\a` {
		t.Errorf("expandEnvVars[0] = %q, want %q", got[0], `D:\Temp\a`)
	}
	if got[1] != `$Recycle.Bin` {
		t.Errorf("expandEnvVars[1] = %q, want $Recycle.Bin (не должен ломаться)", got[1])
	}
}
