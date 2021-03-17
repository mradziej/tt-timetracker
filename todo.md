* change resume: no option = #0, --interactive for interactive use
* New levels:
  - lib.rs only decodes the subcommand, 
    - remaining fns from options go there
  - module subcommand::XXX handles the subcommand
    + add (with resume)
    + report
    + interactive
    + for each:
      + pub(crate) run_xxx() decodes the options (and e.g. creates an entry or so)
      + pub xxx() has a usable public interface that calls sensible pub intermediate steps
      + all these are publicly re-exported in lib.rs (utilities stay in their modules)
      + also: log_parser::Block and FileProxy if part of pub interface
        
  - module entry contains the block types
    - inclusive parsing, writing, validation
    - rename block to entry?
  - module activities reading and writing the acitivity hashmap
  - collector stays

 * log_adder refactoring:
   - rename write_log to subcommand_add
   - move validate_activity into Block
     - this could return whether there is a new shortcut in the tags
   - separate function to create new entry in activities
* for interactive: list yesterday's task
* create a settings struct (with only prefix),
    - implicit is normal on first usage
    - or explicit: from_file(ProxyFile) or new().with_prefix().init() or .init()
      - these check whether already initialized and return a result  
    - can only be initialized once
    - if not initialized before first usage, uses homedir/.tt/config by default  
    - transparently uses lazy_static and config-rs
    - static ref SETTINGS: Settings = Settings::lazy_init() (non-pub fn!)
    - static ref SETTINGS_INITIALIZED = 

* Move FileProxy to separate module fileproxy
