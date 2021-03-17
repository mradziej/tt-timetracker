# tt-timetracker
a command line time tracker with too many features and not much documentation

It's more a project to get my feet wet with rust, though, a co-worker started 
to ask for it. Oh my. This bit of documentation is too little,
and the command line interface has ... ah ... developed historically.

tt-timetracker is a tool to track the time you have spent in projects, for
purposes such as billing customers or reporting your work time. It's supposed
to be a quick command line utility with some support for i3. 

I call it tt.

# It has too many subcommands.

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

# Files

- $HOME/.tt/activities - activities and shortnames
- $HOME/.tt/config - configuration
- $HOME/.tt/<date> (iso format) - log of activities
  
An entry in a log file is always one line.
It can be of the following forms:

- \# comment
- HH:MM activity tag1 tag2 ...
- HH:MM HH_MM activity etc. valid at the second timestamp
- HH:MM really activity - corrects the previous activity

The first timestamp is always the log time.
