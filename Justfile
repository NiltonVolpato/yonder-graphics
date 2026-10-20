# justfile

set shell := ["bash", "-uc"]
set unstable := true
set lists := true
set dotenv-load := true
set dotenv-filename := [".env", ".env.local"]
set dotenv-override := true

# Add the scripts directory to the PATH so we can run custom tools.
export PATH := justfile_directory() + '/scripts:' + env('PATH')

export RUSTFLAGS := '-D warnings'
export RUSTDOCFLAGS := '-D warnings'

CARGO := 'cargo --quiet'
CARGO_NIGHTLY := 'cargo +nightly --quiet'

[default]
_default:
    @just --list

# NOTE: Dependencies are ordered by execution speed: faster checks (formatting, static lints) run first.
[doc('Comprehensive workspace checks: formatting check, clippy, doc, and tests')]
[group('verification')]
verify: fmt clippy doc test

[doc('Same as `verify` but with coverage tests')]
[group('verification')]
verify-full: verify coverage

[doc('Format the Rust code specified in args using nightly rustfmt')]
[group('style')]
fmt +args='--all':
    {{ CARGO_NIGHTLY }} fmt {{ args }}

[doc('Run clippy on all targets, denying warnings')]
[group('style')]
clippy +args='--all-targets':
    mute {{ CARGO }} clippy {{ args }}

[doc('Checks that the code compiles')]
[group('test')]
check +args='--workspace --all-targets':
    mute {{ CARGO }} check {{ args }}

[doc('Run tests on the workspace by default, or individual targets')]
[group('test')]
test +args='--workspace':
    {{ CARGO }} test --quiet {{ args }} --lib --tests

[doc('Build documentation, treating warnings as errors')]
[group('test')]
doc +args='--no-deps':
    {{ CARGO }} doc {{ args }}

export COVERAGE_TARGET := "90"
coverage_args := '--quiet'

[doc('Test coverage greater than {{ COVERAGE_TARGET }}%')]
[group('test')]
coverage +args='--workspace':
    #!/bin/zsh
    print_and_run() { print -P "%B$*%b"; "$@" }
    print_and_run mute {{ CARGO }} llvm-cov {{ coverage_args }} \
        --fail-under-lines {{ COVERAGE_TARGET }} \
        --text \
        --output-path target/llvm-cov-target/coverage.txt \
        test {{ args }}
    exit_status=$?
    if [ $exit_status -ne 0 ]; then
        print
        print -P "    %F{red}%BCoverage failed!%b%f"
        print -P "    You can view the report with: %Bjust coverage-report%b"
        print
        exit $exit_status
    else
        print -P "    %F{green}Coverage passed!%f Report: %Bjust coverage-report%b"
    fi

[group('test')]
coverage-report compact='':
    #!/bin/zsh
    B="$(print -f "%s" -P "%B")"
    b="$(print -f "%s" -P "%b")"
    print -P "%BCoverage Report:%b"
    {{ CARGO }} llvm-cov {{ coverage_args }} report --show-missing-lines \
        | gawk '
        function emoji(coverage) {
            if (coverage >= target) return "✅"
            if (coverage >= 70) return "🟡"
            return "🔴"
        }
        BEGIN {
            target = {{ COVERAGE_TARGET }}
            compact = match("{{ compact }}", /--compact/)
        }
        /^-+$/ {
            section=1
            next
        }
        /^Uncovered Lines:$/ {
            section=2
            next
        }
        section==1 {
            line_coverage[$1]=$10
        }
        section==2 {
            gsub(/'${PWD//\//\\\/}'\/|^\/|:/, "", $1)
            cov = line_coverage[$1] + 0.0
            printf "%s  %.2f%%\t%s\n", emoji(cov), cov, $1
            if (compact) next
            printf "\n"
            gsub(/,/, "", $2)
            start = prev = $2
            for (i = 3; i <= NF; i++) {
                sub(/,/, "", $i)
                if ($i == prev + 1) {
                    prev = $i
                } else {
                    printf "%s, ", (start == prev ? start : start "-" prev)
                    start = prev = $i
                }
            }
            printf "%s\n\n", (start == prev ? start : start "-" prev)
        }
        END {
            total = line_coverage["TOTAL"] + 0.0
            print "'$B'TOTAL: ", total, " (", emoji(total), ")'$b'"
        }'
