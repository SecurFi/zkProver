#!/usr/bin/env bash
set -e

echo Installing SecurFi zkProver...

BASE_DIR=${XDG_CONFIG_HOME:-$HOME}
APP_DIR=${APP_DIR-"$BASE_DIR/.securfi"}
BIN_DIR="$APP_DIR/bin"

mkdir -p $BIN_DIR

# Store the correct profile file (i.e. .profile for bash or .zshenv for ZSH).
case $SHELL in
*/zsh)
    PROFILE=${ZDOTDIR-"$HOME"}/.zshenv
    PREF_SHELL=zsh
    ;;
*/bash)
    PROFILE=$HOME/.bashrc
    PREF_SHELL=bash
    ;;
*/fish)
    PROFILE=$HOME/.config/fish/config.fish
    PREF_SHELL=fish
    ;;
*/ash)
    PROFILE=$HOME/.profile
    PREF_SHELL=ash
    ;;
*)
    echo "SecurFi: could not detect shell, manually add ${BIN_DIR} to your PATH."
    exit 1
    ;;
esac

# Only add if it isn't already in PATH.
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    # Add the SecurFi directory to the path and ensure the old PATH variables remain.
    echo >>$PROFILE && echo "export PATH=\"\$PATH:$BIN_DIR\"" >>$PROFILE
    echo && echo "Detected your preferred shell is ${PREF_SHELL} and added SecurFi to PATH. Run 'source ${PROFILE}' or start a new terminal session to use SecurFi zkProver."
fi

need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
    fi
}

check_cmd() {
    command -v "$1" &>/dev/null
}

say() {
    printf "SecurFi: %s\n" "$1"
}

warn() {
    say "warning: ${1}" >&2
}

err() {
    say "$1" >&2
    exit 1
}

ensure() {
    if ! "$@"; then err "command failed: $*"; fi
}

banner() {
    cat <<"EOF"
  ********                                ******** **
 **//////                                /**///// //
/**         *****   *****  **   ** ******/**       **
/********* **///** **///**/**  /**//**//*/******* /**
////////**/*******/**  // /**  /** /** / /**////  /**
       /**/**//// /**   **/**  /** /**   /**      /**
 ******** //******//***** //******/***   /**      /**
////////   //////  /////   ////// ///    //       //

=============================================================================================
SecurFi - Trustless Security Layer for Onchain Ecosystems
Repo     :  https://github.com/SecurFi/zkProver
Website  :  https://Secur.Fi
Contact  :  https://t.me/SecurFi
EOF
}

install() {
    need_cmd git
    need_cmd cargo

    REPO_URL="https://github.com/SecurFi/zkProver"
    BRANCH="main"
    GIT_DIR="$APP_DIR/git"
    REPO_PATH="$GIT_DIR/zkProver"

    if [ ! -d "$REPO_PATH" ]; then
        ensure mkdir -p "$GIT_DIR"
        cd "$GIT_DIR"
        ensure git clone "$REPO_URL"
    fi

    cd "$REPO_PATH"
    ensure git fetch origin "${BRANCH}:remotes/origin/${BRANCH}"
    ensure git checkout "origin/${BRANCH}"

    GIT_COMMIT=$(git rev-parse HEAD)
    say "installing zkProver (commit $GIT_COMMIT)"

    # build
    GPU=""
    if check_cmd nvcc; then
        GPU="cuda"
    fi
    if [[ "$OSTYPE" =~ ^darwin ]]; then
        GPU="metal"
    fi
    if [ $ZKPROVER_CPU ]; then
        GPU=""
    fi

    if [[ -n "$GPU" ]]; then
        say "build with $GPU"
        ensure cargo build -r -F "$GPU"
    else
        ensure cargo build -r
    fi
    bin="zkProver"
    for try_path in target/release/$bin target/release/$bin.exe; do
        if [ -f "$try_path" ]; then
            [ -e "$BIN_DIR/$bin" ] && warn "overwriting existing $bin in $BIN_DIR"
            mv -f "$try_path" "$BIN_DIR"
        fi
    done
    say "done"
}

banner
install
