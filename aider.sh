#!/usr/bin/env bash

set -e

LMSTUDIO_URL="http://localhost:1234/v1"

if ! command -v jq >/dev/null 2>&1; then
    echo "❌ jq ist nicht installiert."
    echo "Installieren mit: brew install jq"
    exit 1
fi

echo
echo "Verfügbare LM Studio Modelle:"
echo "============================"
echo

MODELS=()
while IFS= read -r line; do
    MODELS+=("$line")
done < <(
    curl -s "$LMSTUDIO_URL/models" | jq -r '.data[].id'
)

if [ ${#MODELS[@]} -eq 0 ]; then
    echo "Keine Modelle gefunden."
    exit 1
fi

for i in "${!MODELS[@]}"; do
    printf "%2d) %s\n" $((i+1)) "${MODELS[$i]}"
done

echo
read -p "Welches Modell möchtest du starten? [1-${#MODELS[@]}]: " CHOICE

if ! [[ "$CHOICE" =~ ^[0-9]+$ ]]; then
    echo "Ungültige Eingabe."
    exit 1
fi

INDEX=$((CHOICE-1))

if [ $INDEX -lt 0 ] || [ $INDEX -ge ${#MODELS[@]} ]; then
    echo "Ungültige Auswahl."
    exit 1
fi

MODEL="${MODELS[$INDEX]}"

echo
echo "Starte Aider mit:"
echo "  $MODEL"
echo

exec aider \
    --model "openai/$MODEL" \
    --openai-api-base "$LMSTUDIO_URL" \
    --openai-api-key dummy \
    --no-auto-commits \
    --no-show-model-warnings
