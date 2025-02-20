#!/bin/sh

# ---------------------------------------------------------------------------
# 1) Ensure Rust is installed.
#    - If rustc is not available, install Rust non-interactively
#      using the official rustup script.
# ---------------------------------------------------------------------------
rustc --version || curl https://sh.rustup.rs -sSf | sh

# ---------------------------------------------------------------------------
# 2) Define environment variables and colors for terminal output.
# ---------------------------------------------------------------------------
NEXUS_HOME="$HOME/.nexus"
REPO_PATH="$NEXUS_HOME/network-api"
GREEN='\033[1;32m'
ORANGE='\033[1;33m'
NC='\033[0m'  # No Color

# Ensure the $NEXUS_HOME directory exists.
[ -d "$NEXUS_HOME" ] || mkdir -p "$NEXUS_HOME"

# ---------------------------------------------------------------------------
# 3) Display a message if we're interactive (NONINTERACTIVE is not set) and
#    the $NODE_ID is not a 28-character ID.
# ---------------------------------------------------------------------------
if [ -z "$NONINTERACTIVE" ] && [ "${#NODE_ID}" -ne "28" ]; then
    echo ""
    echo "${ORANGE}The Nexus network is currently in Testnet II. You can now earn Nexus Points.${NC}"
    echo ""
fi

# ---------------------------------------------------------------------------
# 4) Prompt the user to agree to the Nexus Beta Terms of Use if we're in an
#    interactive mode and no node-id file exists.
# ---------------------------------------------------------------------------
while [ -z "$NONINTERACTIVE" ] && [ ! -f "$NEXUS_HOME/node-id" ]; do
    read -p "Do you agree to the Nexus Beta Terms of Use (https://nexus.xyz/terms-of-use)? (Y/n) " yn </dev/tty
    echo ""

    case $yn in
        [Nn]* ) 
            echo ""
            exit;;
        [Yy]* ) 
            echo ""
            break;;
        "" ) 
            echo ""
            break;;
        * ) 
            echo "Please answer yes or no."
            echo "";;
    esac
done

# ---------------------------------------------------------------------------
# 5) Check for 'git' availability. If not found, prompt the user to install it.
# ---------------------------------------------------------------------------
git --version 2>&1 >/dev/null
GIT_IS_AVAILABLE=$?
if [ "$GIT_IS_AVAILABLE" != 0 ]; then
  echo "Git is not installed. Please install it and try again."
  exit 1
fi

# ---------------------------------------------------------------------------
# 6) Clone or update the network-api repository in $NEXUS_HOME.
# ---------------------------------------------------------------------------
if [ -d "$REPO_PATH" ]; then
    echo "$REPO_PATH exists. Checking remote repository."
    (
      cd "$REPO_PATH" || exit
      git fetch origin main
      LOCAL_COMMIT=$(git rev-parse HEAD)
      REMOTE_COMMIT=$(git rev-parse origin/main)

      if [ "$LOCAL_COMMIT" != "$REMOTE_COMMIT" ]; then
          echo "Updating repository..."
          git pull origin main --rebase
      else
          echo "Already up to date."
      fi
    )
else
    (
      cd "$NEXUS_HOME" || exit
      git clone https://github.com/enolife/network-api
    )
fi

# ---------------------------------------------------------------------------
# 7) Run the Rust CLI in interactive mode.
# ---------------------------------------------------------------------------
(
  cd "$REPO_PATH/clients/cli" || exit
  cargo run -r -- start --env beta
) < /dev/tty
