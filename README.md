# tt-timetracker
a command line time tracker with too many features and not much documentation

It's more a project to get my feet wet with rust, though, a co-worker started 
to ask for it. Oh my. This bit of documentation is too little,
and the command line interface has ... ah ... developed historically.

This is at a pre-alpha stage. As far as a I know,
nobody except the author has actually used it.

# Overview
tt-timetracker is a tool to track the time you have spent in projects, for
purposes such as billing customers or reporting your work time. It's supposed
to be a quick command line utility with some support for i3. 

I call it tt.

## It has too many subcommands.

- tt: add a log entry interactively
- tt add: add a log entry
- tt edit: edit the log file, activities or configuration
- tt report: create a report
- tt is-active: is any activity ongoing? For scripts.
- tt list: lists all available shortnames for activities
- tt resume: resume the previous activity (stackingly)
- tt watch-i3: watch which i3 workspaces are in focus,
     give titles and keep a log of activities
     
Some of these subcommands have a --help option. Some of the help messages
are helpful.

## Files

- $HOME/.tt/activities - activities and shortnames
- $HOME/.tt/config - configuration
- $HOME/.tt/<date> (iso format) - log of activities
  
An entry in a log file is always one line.
It can be of the following forms:

- `\# comment`
- `HH:MM activity tag1 tag2 ...`
- `HH:MM HH_MM activity` etc. valid at the second timestamp
- `HH:MM really activity` - corrects the previous activity

The first timestamp is always the log time.

# Getting Started
## Building
- Clone the git repository and cd into it
- cargo build
- you will find the binary in `target/debug/timetracker`

I usually symlink it to `~/bin/tt` and will call it `tt` here.

For now, stay with the debug version. You deal
with unreleased pre-alpha software.

## Initialization and Configuration
```
mkdir ~/.tt
vi ~/.tt/config.toml
```
There you should set the prefix of you most often used JIRA queue or something similar, as 
default prefix. `tt` will add it implicitly if you enter a purely numeric activity.
Add the following line to the configuration:
```
prefix = "JIRA"
```

You can now configure your current set of activities, this is in a different file:
```
tt edit -a
```
Enter your current activities in the form
```
JIRA-123 shortname
```

This will result in a file `~/.tt/activities`.

You can then refer to the shortname instead of the JIRA id.

In case you are not interested in JIRA ids or similiar,
you can also add activities just by name.

## Logging activity
If you start an activity, add it to the log:
```
tt add <activity>
```

You can leave out the `add` unless it collides with
one of the other subcommands. If the activity
is not listed in the activities file, timetracker
will refuse to add it. You can still add it by using

```
tt add +<activity>
``` 

This will add a line in the activity log file `~/.tt/<date>`:
`<timestamp> <activity>`

You can edit the log file at any time, just keep the format.
There's even a subcommand for it:

```
tt edit
```

## Getting reports
```
tt report
```

will give you a report on the day

```
tt report -y
```

will give you a report on yesterday

```
tt report -w
```

will give you a report on the week

There are different formats and lots of other options.
For help, try 
```
tt report --help
```

This is still evolving.


## Logging work continued
tt add has nice options to deal with the many
interruptions and task changes we regrettably have.

Please note that tt will bail out with an error if you mix up
the time sequence, like adding an entry for 10:15 after an entry for 10:00. You then
need to edit the log file and fix this. This is still an area of future improvement.

### Start not now but at an earlier time
When the activity started earlier:

```
tt add -t HH:MM <activity>
```

### Start a few minutes before now
Or, for "it started xxx minutes ago"

```
tt add --ago MM <activity>
```

### Worked on a different activity than planned
If it turns out that you actually did not work
on what you thought you'd do, you can change this:

```
tt add --really <activity>
```

... this means that actually worked on the new acitvitiy

### Work started on a different time than planned
Or if the real start of the activity turns out to be a different time:

```
tt add --really -t HH:MM
```

### Resume the previous activity, like in a stack
```
tt resume
```

This will bring you back on the previous activity. It stacks.

### Edit the logfile

And don't forget, you can always edit the log file afterwards if it
turns out that you need more complicated changes.

```
tt edit
```
 
## Special log entries

There are a few special "activities":

### internal activities
`_<something>`

Everything that starts with an underscore is an "internal" activity. tt report will distribute
any time spent on an internal acitivity evenly on the other activities of that day.

This is great if you have activities that you cannot report directly, but are worktime.

Activities that start with an underscore are always accepted by tt add.

### break
`break` means you start a break (or finish work for today)

### start
`start` means that you start something new but don't know what it is, like: the phone rings ...

You can later use `--really` to specify what you did.

The interactive mode and i3 integration understand that "start" started something and will imply `--really`
when adding a new activity.

## Interactive use
Just type `tt`. It will present you with a menu of the activities that you have configured, or if you
do not choose any of this, will present you all activities from the current day.

## i3 integration
This is nice if you use the i3 window manager and want to use dedicated workspaces for your activities.

tt watch-i3 starts a loop that will continue forever and every few seconds which workspace is in focus.

If it finds that the same workspace is in focus for more than a configurable time 
(it's a bit more complicated, for details see below), it will either

- give the workspace a title according to the current activity
- or, if the workspace has a title, add a new log entry with the corresponding workspace title

### Details

There are two configurable settings:

- `watch-i3.granularity` (in seconds, default is 10)
- `watch-i3.timebox` (in seconds, default is 120)

tt will

- check the current activity (and reset the focus counters if it changes)
- check the currently focused workspace and count how often each workspace is focused
- check wether the time that the workspace in focus has been in focus since the last reset
  for longer than timebox, then it will act (see below) and reset the focus counters again
- then sleep for as many seconds as the granularity implies.

If a workspace has been in focus for long enough, tt will

- do nothing if there is no current activity today or the previous activity is "break"
- set the workspace title to the name of the current activity if
  - the workspace does not have a name yet
  - and there is no other workspace on the same output with the name of the current activity
- but if the workspace has a title, it will log that you started (timebox seconds ago) 
  a new activity (as the name of the workspace suggests).
  
It is planned that the detection of non-activity is added so that breaks can be detected.
