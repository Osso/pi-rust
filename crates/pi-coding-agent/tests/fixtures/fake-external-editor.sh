#!/bin/sh
# Fake external editor used by the external-editor behavioral test.
set -u
capture="$1"
shift
for last; do :; done
file="$last"
if [ -z "$capture" ] || [ -z "$file" ]; then
  exit 1
fi
directory=$(dirname "$file")
content=$(cat "$file")
entries=$(ls -A "$directory" | tr '\n' ' ')
mode=$(stat -c '%a' "$directory")
printf '{"filePath":"%s","content":"%s","entries":["%s"],"directoryMode":%s}' \
  "$file" "$content" "$(printf '%s' "$entries" | sed 's/ $//')" "$((0$mode))" >"$capture"
case " $* " in
  *" --fail "*) exit 1 ;;
esac
case " $* " in
  *" --empty "*) : >"$file" ;;
  *) printf 'edited\n' >"$file" ;;
esac
