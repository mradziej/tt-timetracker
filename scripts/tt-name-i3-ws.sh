#!/bin/bash
# set name
WS=$(python3 -c "import json; print(next(filter(lambda w: w['focused'], json.loads('$(i3-msg -t get_workspaces)')))['num'])")
ACTIVITY="$(tt report --format=activity)"
if [ "$ACTIVITY" == _start ] || [ "$ACTIVITY" == break ]
then
	i3-msg "rename workspace to $WS"
else
	i3-msg "rename workspace to \"$WS: $ACTIVITY\""
fi
