#!/bin/bash
# Predator Sense - Abra este arquivo para instalar
# Funciona em qualquer distribuição Linux

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Detect terminal emulator
TERM_CMD=""
for t in x-terminal-emulator gnome-terminal xfce4-terminal konsole mate-terminal lxterminal xterm; do
    if command -v "$t" &>/dev/null; then
        TERM_CMD="$t"
        break
    fi
done

if [ -z "$TERM_CMD" ]; then
    # No terminal found, try running directly
    if [ "$EUID" -ne 0 ]; then
        # Try graphical sudo
        if command -v pkexec &>/dev/null; then
            pkexec bash "$SCRIPT_DIR/setup.sh"
        else
            echo "Execute: sudo bash $SCRIPT_DIR/setup.sh"
        fi
    else
        bash "$SCRIPT_DIR/setup.sh"
    fi
    exit 0
fi

# Open terminal with the setup
case "$TERM_CMD" in
    gnome-terminal)
        gnome-terminal -- bash -c "cd '$SCRIPT_DIR' && sudo bash ./setup.sh; echo; echo 'Pressione ENTER para fechar...'; read" ;;
    xfce4-terminal)
        xfce4-terminal -e "bash -c \"cd '$SCRIPT_DIR' && sudo bash ./setup.sh; echo; read -p 'Pressione ENTER para fechar...'\"" ;;
    konsole)
        konsole -e bash -c "cd '$SCRIPT_DIR' && sudo bash ./setup.sh; echo; read -p 'Pressione ENTER para fechar...'" ;;
    mate-terminal)
        mate-terminal -e "bash -c \"cd '$SCRIPT_DIR' && sudo bash ./setup.sh; echo; read -p 'Pressione ENTER para fechar...'\"" ;;
    *)
        $TERM_CMD -e "bash -c \"cd '$SCRIPT_DIR' && sudo bash ./setup.sh; echo; read -p 'Pressione ENTER para fechar...'\"" ;;
esac
