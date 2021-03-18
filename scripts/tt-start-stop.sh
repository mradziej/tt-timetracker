#!/bin/bash

MIN_IDLE=60
WARN_AFTER=600
LOCK_AFTER=660
LOG=~/stuff/tt-assist.log
PREV_IDLE=$(($(xprintidle)/1000))
PREV_EPOCH=$(date +%s)
SLEEP=60
while /bin/true
do
    TIME=$(date +%H:%M)
    IDLE=$(($(xprintidle)/1000))
    EPOCH=$(date +%s)
    MAX_IDLE=$((EPOCH - PREV_EPOCH + IDLE + 2))

    # xprintidle sometimes reports bogus values.
    if [ $IDLE -gt $MAX_IDLE ]
    then
        echo "$TIME: IDLE is bogus, was $IDLE, setting to $MAX_IDLE"
        IDLE="$MAX_IDLE"
    fi
    
    if [ $IDLE -ge $LOCK_AFTER ]
    then
        if tt is-active
        then
            tt add --ago $((LOCK_AFTER / 60)) break auto
            ACTIVE=1
        else
            ACTIVE=0
        fi
        echo "$TIME: Locking now."
        i3lock -c 000055 -n

        if [ $ACTIVE -eq 1 ]
        then
            tt resume
        fi
        echo "$TIME: Resumed."
        echo
        echo
        SLEEP=$WARN_AFTER
        IDLE=0
    elif [ $IDLE -ge $WARN_AFTER ]
    then
        SLEEP=$((LOCK_AFTER - IDLE + 1))
        echo "---------------------------------"
        echo "$TIME: LOCKING in $SLEEP seconds!"
        zenity --warning --text "Screen will be locked without activity" --timeout "$SLEEP" --title "Screen Locker"
        TIME=$(date +%H:%M)
        if [ $IDLE -lt $WARN_AFTER ]
        then
            echo "$TIME: Cancelled."
            echo
            echo
        fi
        IDLE=$((IDLE + SLEEP))
        SLEEP=1
    else
        SLEEP="$((WARN_AFTER - IDLE + 1))"
    fi
    PREV_EPOCH=$EPOCH
    PREV_IDLE=$IDLE
    sleep "$SLEEP"
    
done
