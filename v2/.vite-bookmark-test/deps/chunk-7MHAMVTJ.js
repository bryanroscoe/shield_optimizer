import {
  clsx
} from "./chunk-KDVGFZWC.js";
import {
  append,
  assign_nodes,
  can_delegate_event,
  createClassComponent,
  create_fragment_from_html,
  create_trusted_html,
  hash,
  is_capture_event,
  is_raw_text_element,
  is_void,
  normalize_attribute,
  sanitize_location,
  set_should_intro,
  should_intro
} from "./chunk-764NIRZ7.js";
import {
  create_event,
  delegate,
  delegated
} from "./chunk-YY3KXDTM.js";
import {
  ATTACHMENT_KEY,
  ATTRIBUTES_CACHE,
  BLOCK_EFFECT,
  BRANCH_EFFECT,
  Batch,
  CLASS_CACHE,
  COMMENT_NODE,
  DESTROYED,
  DESTROYING,
  DOCUMENT_FRAGMENT_NODE,
  EACH_INDEX_REACTIVE,
  EACH_IS_ANIMATED,
  EACH_IS_CONTROLLED,
  EACH_ITEM_IMMUTABLE,
  EACH_ITEM_REACTIVE,
  EFFECT_OFFSCREEN,
  EFFECT_TRANSPARENT,
  ELEMENT_NODE,
  FILENAME,
  FORM_RESET_HANDLER,
  HEAD_EFFECT,
  HMR,
  HMR_ANCHOR,
  HYDRATION_END,
  HYDRATION_ERROR,
  HYDRATION_START,
  HYDRATION_START_ELSE,
  INERT,
  IS_XHTML,
  LEGACY_PROPS,
  LOADING_ATTR_SYMBOL,
  NAMESPACE_HTML,
  NAMESPACE_MATHML,
  NAMESPACE_SVG,
  PROPS_IS_BINDABLE,
  PROPS_IS_IMMUTABLE,
  PROPS_IS_LAZY_INITIAL,
  PROPS_IS_RUNES,
  PROPS_IS_UPDATED,
  REACTION_RAN,
  STATE_SYMBOL,
  STYLE_CACHE,
  TRANSITION_GLOBAL,
  TRANSITION_IN,
  TRANSITION_OUT,
  UNINITIALIZED,
  active_effect,
  active_reaction,
  add_form_reset_listener,
  array_from,
  autofocus,
  bind_invalid_checkbox_value,
  block,
  branch,
  capture,
  capture_store_binding,
  clear_text_content,
  component_api_changed,
  component_api_invalid_new,
  component_context,
  create_comment,
  create_element,
  create_text,
  current_batch,
  deep_read_state,
  define_property,
  derived2 as derived,
  derived_safe_equal,
  destroy_effect,
  dev_current_component_function,
  dev_stack,
  dynamic_void_element_content,
  each_key_duplicate,
  each_key_volatile,
  eager_effect,
  effect,
  effect_root,
  experimental_async_required,
  flatten,
  flushSync,
  get2 as get,
  get_abort_signal_outside_reaction,
  get_descriptor,
  get_descriptors,
  get_error,
  get_first_child,
  get_next_sibling,
  get_prototype_of,
  has_own_property,
  hydratable_missing_but_required,
  hydrate_next,
  hydrate_node,
  hydrating,
  internal_set,
  invalid_snippet,
  invalid_snippet_arguments,
  is,
  is_array,
  is_destroying_effect,
  is_flushing_sync,
  is_function,
  is_promise,
  is_runes,
  lifecycle_legacy_only,
  lifecycle_outside_component,
  listen,
  listen_to_event_and_reset_event,
  managed,
  move_effect,
  mutable_source,
  noop,
  object_keys,
  pause_effect,
  previous_batch,
  props_invalid_value,
  props_rest_readonly,
  proxy,
  queue_micro_task,
  read_hydration_instruction,
  remove_effect_dom,
  render_effect,
  reset,
  resume_effect,
  run,
  run_after_blockers,
  run_all,
  rune_outside_svelte,
  safe_not_equal,
  set,
  set_active_effect,
  set_active_reaction,
  set_dev_current_component_function,
  set_hydrate_node,
  set_hydrating,
  should_defer_append,
  skip_nodes,
  snapshot,
  snippet_without_render_tag,
  source,
  store_invalid_shape,
  svelte_element_invalid_this_value,
  tag,
  teardown,
  template_effect,
  tick,
  unset_context,
  untrack,
  update,
  user_effect,
  user_pre_effect,
  validate_effect,
  without_reactive_context
} from "./chunk-RA5JWBT7.js";
import {
  assignment_value_stale,
  binding_property_non_reactive,
  console_log_state,
  hydratable_missing_but_expected,
  hydration_attribute_changed,
  hydration_html_changed,
  hydration_mismatch,
  invalid_raw_snippet_render,
  ownership_invalid_binding,
  ownership_invalid_mutation,
  select_multiple_invalid_value
} from "./chunk-RDQBLHQD.js";
import {
  true_default
} from "./chunk-2GRV7DLA.js";
import {
  async_mode_flag,
  legacy_mode_flag
} from "./chunk-6ZD2PQMF.js";
import {
  __privateAdd,
  __privateGet,
  __privateMethod,
  __privateSet,
  __publicField
} from "./chunk-UGBVNEQM.js";

// node_modules/svelte/src/internal/client/dev/assign.js
function compare(a, b, property, location) {
  if (a !== b && typeof b === "object" && STATE_SYMBOL in b) {
    assignment_value_stale(
      property,
      /** @type {string} */
      sanitize_location(location)
    );
  }
  return a;
}
function assign(object, property, operator, rhs, location) {
  return compare(
    operator === "=" ? object[property] = rhs : operator === "&&=" ? object[property] && (object[property] = rhs()) : operator === "||=" ? object[property] || (object[property] = rhs()) : operator === "??=" ? object[property] ?? (object[property] = rhs()) : null,
    untrack(() => object[property]),
    property,
    location
  );
}
async function assign_async(object, property, operator, rhs, location) {
  return compare(
    operator === "=" ? object[property] = await rhs : operator === "&&=" ? object[property] && (object[property] = await rhs()) : operator === "||=" ? object[property] || (object[property] = await rhs()) : operator === "??=" ? object[property] ?? (object[property] = await rhs()) : null,
    untrack(() => object[property]),
    property,
    location
  );
}

// node_modules/svelte/src/internal/client/dev/css.js
var all_styles = /* @__PURE__ */ new Map();
function register_style(hash2, style) {
  var styles = all_styles.get(hash2);
  if (!styles) {
    styles = /* @__PURE__ */ new Set();
    all_styles.set(hash2, styles);
  }
  styles.add(style);
}
function cleanup_styles(hash2) {
  var styles = all_styles.get(hash2);
  if (!styles) return;
  for (const style of styles) {
    style.remove();
  }
  all_styles.delete(hash2);
}

// node_modules/svelte/src/internal/client/dev/elements.js
function add_locations(fn, filename, locations) {
  return (...args) => {
    const dom = fn(...args);
    var node = hydrating ? dom : dom.nodeType === DOCUMENT_FRAGMENT_NODE ? dom.firstChild : dom;
    assign_locations(node, filename, locations);
    return dom;
  };
}
function assign_location(element2, filename, location) {
  element2.__svelte_meta = {
    parent: dev_stack,
    loc: { file: filename, line: location[0], column: location[1] }
  };
  if (location[2]) {
    assign_locations(element2.firstChild, filename, location[2]);
  }
}
function assign_locations(node, filename, locations) {
  var i = 0;
  var depth = 0;
  while (node && i < locations.length) {
    if (hydrating && node.nodeType === COMMENT_NODE) {
      var comment2 = (
        /** @type {Comment} */
        node
      );
      if (comment2.data[0] === HYDRATION_START) depth += 1;
      else if (comment2.data[0] === HYDRATION_END) depth -= 1;
    }
    if (depth === 0 && node.nodeType === ELEMENT_NODE) {
      assign_location(
        /** @type {Element} */
        node,
        filename,
        locations[i++]
      );
    }
    node = node.nextSibling;
  }
}

// node_modules/svelte/src/internal/client/dev/hmr.js
function hmr(fn) {
  const current = source(fn);
  function wrapper(initial_anchor, props) {
    let component2 = {};
    let instance = {};
    let effect2;
    let ran = false;
    let anchor = initial_anchor;
    block(() => {
      if (component2 === (component2 = get(current))) {
        return;
      }
      if (effect2) {
        for (var k in instance) delete instance[k];
        destroy_effect(effect2);
      }
      effect2 = branch(() => {
        anchor = /** @type {any} */
        anchor[HMR_ANCHOR] ?? anchor;
        if (ran) set_should_intro(false);
        var result = (
          // @ts-expect-error
          new.target ? new component2(anchor, props) : component2(anchor, props)
        );
        if (result) {
          Object.defineProperties(instance, Object.getOwnPropertyDescriptors(result));
        }
        if (ran) set_should_intro(true);
      });
      active_effect.nodes = effect2.nodes;
    }, EFFECT_TRANSPARENT);
    ran = true;
    if (hydrating) {
      anchor = hydrate_node;
    }
    return instance;
  }
  wrapper[FILENAME] = fn[FILENAME];
  wrapper[HMR] = {
    fn,
    current,
    update: (incoming) => {
      set(wrapper[HMR].current, incoming[HMR].fn);
      incoming[HMR].current = wrapper[HMR].current;
    }
  };
  return wrapper;
}

// node_modules/svelte/src/internal/client/dev/ownership.js
function create_ownership_validator(props) {
  var _a, _b, _c;
  const component2 = (_a = component_context) == null ? void 0 : _a.function;
  const parent = (_c = (_b = component_context) == null ? void 0 : _b.p) == null ? void 0 : _c.function;
  return {
    /**
     * @param {string} prop
     * @param {any[]} path
     * @param {any} result
     * @param {number} line
     * @param {number} column
     */
    mutation: (prop2, path, result, line, column) => {
      const name = path[0];
      if (is_bound_or_unset(props, name) || !parent) {
        return result;
      }
      let value = props;
      for (let i = 0; i < path.length - 1; i++) {
        value = value[path[i]];
        if (!(value == null ? void 0 : value[STATE_SYMBOL])) {
          return result;
        }
      }
      const location = sanitize_location(`${component2[FILENAME]}:${line}:${column}`);
      ownership_invalid_mutation(name, location, prop2, parent[FILENAME]);
      return result;
    },
    /**
     * @param {any} key
     * @param {any} child_component
     * @param {() => any} value
     */
    binding: (key2, child_component, value) => {
      var _a2;
      if (!is_bound_or_unset(props, key2) && parent && ((_a2 = value()) == null ? void 0 : _a2[STATE_SYMBOL])) {
        ownership_invalid_binding(
          component2[FILENAME],
          key2,
          child_component[FILENAME],
          parent[FILENAME]
        );
      }
    }
  };
}
function is_bound_or_unset(props, prop_name) {
  var _a;
  const is_entry_props = STATE_SYMBOL in props || LEGACY_PROPS in props;
  return !!((_a = get_descriptor(props, prop_name)) == null ? void 0 : _a.set) || is_entry_props && prop_name in props || !(prop_name in props);
}

// node_modules/svelte/src/internal/client/dev/legacy.js
function check_target(target) {
  if (target) {
    component_api_invalid_new(target[FILENAME] ?? "a component", target.name);
  }
}
function legacy_api() {
  var _a;
  const component2 = (_a = component_context) == null ? void 0 : _a.function;
  function error(method) {
    component_api_changed(method, component2[FILENAME]);
  }
  return {
    $destroy: () => error("$destroy()"),
    $on: () => error("$on(...)"),
    $set: () => error("$set(...)")
  };
}

// node_modules/svelte/src/internal/client/dev/inspect.js
function inspect(get_value, inspector, show_stack = false) {
  validate_effect("$inspect");
  let initial = true;
  let error = (
    /** @type {any} */
    UNINITIALIZED
  );
  eager_effect(() => {
    error = UNINITIALIZED;
    try {
      var value = get_value();
    } catch (e) {
      error = e;
      return;
    }
    var snap = snapshot(value, true, true);
    untrack(() => {
      if (show_stack) {
        inspector(...snap);
        if (!initial) {
          const stack = get_error("$inspect(...)");
          if (stack) {
            console.groupCollapsed("stack trace");
            console.log(stack);
            console.groupEnd();
          }
        }
      } else {
        inspector(initial ? "init" : "update", ...snap);
      }
    });
    initial = false;
  });
  render_effect(() => {
    try {
      get_value();
    } catch {
    }
    if (error !== UNINITIALIZED) {
      console.error(error);
      error = UNINITIALIZED;
    }
  });
}

// node_modules/svelte/src/internal/client/dom/blocks/async.js
function async(node, blockers = [], expressions = [], fn) {
  var was_hydrating = hydrating;
  var end = null;
  if (was_hydrating) {
    hydrate_next();
    end = skip_nodes(false);
  }
  if (expressions.length === 0 && blockers.every((b) => b.settled)) {
    fn(node);
    if (was_hydrating) {
      set_hydrate_node(end);
    }
    return;
  }
  if (was_hydrating) {
    var previous_hydrate_node = hydrate_node;
    set_hydrate_node(end);
  }
  flatten(blockers, [], expressions, (values) => {
    if (was_hydrating) {
      set_hydrating(true);
      set_hydrate_node(previous_hydrate_node);
    }
    try {
      for (const d of values) get(d);
      fn(node, ...values);
    } finally {
      if (was_hydrating) {
        set_hydrating(false);
      }
    }
  });
}

// node_modules/svelte/src/internal/client/dev/validation.js
function validate_snippet_args(anchor, ...args) {
  if (typeof anchor !== "object" || !(anchor instanceof Node)) {
    invalid_snippet_arguments();
  }
  for (let arg of args) {
    if (typeof arg !== "function") {
      invalid_snippet_arguments();
    }
  }
}

// node_modules/svelte/src/internal/client/dom/blocks/branches.js
var _batches, _onscreen, _offscreen, _outroing, _transition, _commit, _discard;
var BranchManager = class {
  /**
   * @param {TemplateNode} anchor
   * @param {boolean} transition
   */
  constructor(anchor, transition2 = true) {
    /** @type {TemplateNode} */
    __publicField(this, "anchor");
    /** @type {Map<Batch, Key>} */
    __privateAdd(this, _batches, /* @__PURE__ */ new Map());
    /**
     * Map of keys to effects that are currently rendered in the DOM.
     * These effects are visible and actively part of the document tree.
     * Example:
     * ```
     * {#if condition}
     * 	foo
     * {:else}
     * 	bar
     * {/if}
     * ```
     * Can result in the entries `true->Effect` and `false->Effect`
     * @type {Map<Key, Effect>}
     */
    __privateAdd(this, _onscreen, /* @__PURE__ */ new Map());
    /**
     * Similar to #onscreen with respect to the keys, but contains branches that are not yet
     * in the DOM, because their insertion is deferred.
     * @type {Map<Key, Branch>}
     */
    __privateAdd(this, _offscreen, /* @__PURE__ */ new Map());
    /**
     * Keys of effects that are currently outroing
     * @type {Set<Key>}
     */
    __privateAdd(this, _outroing, /* @__PURE__ */ new Set());
    /**
     * Whether to pause (i.e. outro) on change, or destroy immediately.
     * This is necessary for `<svelte:element>`
     */
    __privateAdd(this, _transition, true);
    /**
     * @param {Batch} batch
     */
    __privateAdd(this, _commit, (batch) => {
      if (!__privateGet(this, _batches).has(batch)) return;
      var key2 = (
        /** @type {Key} */
        __privateGet(this, _batches).get(batch)
      );
      var onscreen = __privateGet(this, _onscreen).get(key2);
      if (onscreen) {
        resume_effect(onscreen);
        __privateGet(this, _outroing).delete(key2);
      } else {
        var offscreen = __privateGet(this, _offscreen).get(key2);
        if (offscreen) {
          __privateGet(this, _onscreen).set(key2, offscreen.effect);
          __privateGet(this, _offscreen).delete(key2);
          if (true_default) {
            offscreen.fragment.lastChild[HMR_ANCHOR] = this.anchor;
          }
          offscreen.fragment.lastChild.remove();
          this.anchor.before(offscreen.fragment);
          onscreen = offscreen.effect;
        }
      }
      for (const [b, k] of __privateGet(this, _batches)) {
        __privateGet(this, _batches).delete(b);
        if (b === batch) {
          break;
        }
        const offscreen2 = __privateGet(this, _offscreen).get(k);
        if (offscreen2) {
          destroy_effect(offscreen2.effect);
          __privateGet(this, _offscreen).delete(k);
        }
      }
      for (const [k, effect2] of __privateGet(this, _onscreen)) {
        if (k === key2 || __privateGet(this, _outroing).has(k)) continue;
        const on_destroy = () => {
          const keys = Array.from(__privateGet(this, _batches).values());
          if (keys.includes(k)) {
            var fragment = document.createDocumentFragment();
            move_effect(effect2, fragment);
            fragment.append(create_text());
            __privateGet(this, _offscreen).set(k, { effect: effect2, fragment });
          } else {
            destroy_effect(effect2);
          }
          __privateGet(this, _outroing).delete(k);
          __privateGet(this, _onscreen).delete(k);
        };
        if (__privateGet(this, _transition) || !onscreen) {
          __privateGet(this, _outroing).add(k);
          pause_effect(effect2, on_destroy, false);
        } else {
          on_destroy();
        }
      }
    });
    /**
     * @param {Batch} batch
     */
    __privateAdd(this, _discard, (batch) => {
      __privateGet(this, _batches).delete(batch);
      const keys = Array.from(__privateGet(this, _batches).values());
      for (const [k, branch2] of __privateGet(this, _offscreen)) {
        if (!keys.includes(k)) {
          destroy_effect(branch2.effect);
          __privateGet(this, _offscreen).delete(k);
        }
      }
    });
    this.anchor = anchor;
    __privateSet(this, _transition, transition2);
  }
  /**
   *
   * @param {any} key
   * @param {null | ((target: TemplateNode) => void)} fn
   */
  ensure(key2, fn) {
    var batch = (
      /** @type {Batch} */
      current_batch
    );
    var defer = should_defer_append();
    if (fn && !__privateGet(this, _onscreen).has(key2) && !__privateGet(this, _offscreen).has(key2)) {
      if (defer) {
        var fragment = document.createDocumentFragment();
        var target = create_text();
        fragment.append(target);
        __privateGet(this, _offscreen).set(key2, {
          effect: branch(() => fn(target)),
          fragment
        });
      } else {
        __privateGet(this, _onscreen).set(
          key2,
          branch(() => fn(this.anchor))
        );
      }
    }
    __privateGet(this, _batches).set(batch, key2);
    if (defer) {
      for (const [k, effect2] of __privateGet(this, _onscreen)) {
        if (k === key2) {
          batch.unskip_effect(effect2);
        } else {
          batch.skip_effect(effect2);
        }
      }
      for (const [k, branch2] of __privateGet(this, _offscreen)) {
        if (k === key2) {
          batch.unskip_effect(branch2.effect);
        } else {
          batch.skip_effect(branch2.effect);
        }
      }
      batch.oncommit(__privateGet(this, _commit));
      batch.ondiscard(__privateGet(this, _discard));
    } else {
      if (hydrating) {
        this.anchor = hydrate_node;
      }
      __privateGet(this, _commit).call(this, batch);
    }
  }
};
_batches = new WeakMap();
_onscreen = new WeakMap();
_offscreen = new WeakMap();
_outroing = new WeakMap();
_transition = new WeakMap();
_commit = new WeakMap();
_discard = new WeakMap();

// node_modules/svelte/src/internal/client/dom/blocks/await.js
var PENDING = 0;
var THEN = 1;
var CATCH = 2;
function await_block(node, get_input, pending_fn, then_fn, catch_fn) {
  if (hydrating) {
    hydrate_next();
  }
  var runes = is_runes();
  var v = (
    /** @type {V} */
    UNINITIALIZED
  );
  var value = runes ? source(v) : mutable_source(v, false, false);
  var error = runes ? source(v) : mutable_source(v, false, false);
  if (true_default) {
    value.label = "{#await ...} value";
    error.label = "{#await ...} error";
  }
  var branches = new BranchManager(node);
  block(() => {
    var batch = (
      /** @type {Batch} */
      current_batch
    );
    var input = get_input();
    var destroyed = false;
    let mismatch = hydrating && is_promise(input) === (node.data === HYDRATION_START_ELSE);
    if (mismatch) {
      set_hydrate_node(skip_nodes());
      set_hydrating(false);
    }
    if (is_promise(input)) {
      var restore = capture();
      var resolved = false;
      const resolve = (fn) => {
        if (destroyed) return;
        resolved = true;
        restore(false);
        if (current_batch === batch) {
          batch.deactivate();
        }
        Batch.ensure();
        try {
          fn();
        } finally {
          unset_context(false);
          if (!is_flushing_sync) flushSync();
        }
      };
      input.then(
        (v2) => {
          resolve(() => {
            internal_set(value, v2);
            branches.ensure(THEN, then_fn && ((target) => then_fn(target, value)));
          });
        },
        (e) => {
          resolve(() => {
            internal_set(error, e);
            branches.ensure(CATCH, catch_fn && ((target) => catch_fn(target, error)));
            if (!catch_fn) {
              throw error.v;
            }
          });
        }
      );
      if (hydrating) {
        branches.ensure(PENDING, pending_fn);
      } else {
        queue_micro_task(() => {
          if (!resolved) {
            resolve(() => {
              branches.ensure(PENDING, pending_fn);
            });
          }
        });
      }
    } else {
      internal_set(value, input);
      branches.ensure(THEN, then_fn && ((target) => then_fn(target, value)));
    }
    if (mismatch) {
      set_hydrating(true);
    }
    return () => {
      destroyed = true;
    };
  });
}

// node_modules/svelte/src/internal/client/dom/blocks/if.js
function if_block(node, fn, elseif = false) {
  var marker;
  if (hydrating) {
    marker = hydrate_node;
    hydrate_next();
  }
  var branches = new BranchManager(node);
  var flags = elseif ? EFFECT_TRANSPARENT : 0;
  function update_branch(key2, fn2) {
    if (hydrating) {
      var data = read_hydration_instruction(
        /** @type {TemplateNode} */
        marker
      );
      if (key2 !== parseInt(data.substring(1))) {
        var anchor = skip_nodes();
        set_hydrate_node(anchor);
        branches.anchor = anchor;
        set_hydrating(false);
        branches.ensure(key2, fn2);
        set_hydrating(true);
        return;
      }
    }
    branches.ensure(key2, fn2);
  }
  block(() => {
    var has_branch = false;
    fn((fn2, key2 = 0) => {
      has_branch = true;
      update_branch(key2, fn2);
    });
    if (!has_branch) {
      update_branch(-1, null);
    }
  }, flags);
}

// node_modules/svelte/src/internal/client/dom/blocks/key.js
var NAN = Symbol("NaN");
function key(node, get_key, render_fn) {
  if (hydrating) {
    hydrate_next();
  }
  var branches = new BranchManager(node);
  var legacy = !is_runes();
  block(() => {
    var key2 = get_key();
    if (key2 !== key2) {
      key2 = /** @type {any} */
      NAN;
    }
    if (legacy && key2 !== null && typeof key2 === "object") {
      key2 = /** @type {V} */
      {};
    }
    branches.ensure(key2, render_fn);
  });
}

// node_modules/svelte/src/internal/client/dom/blocks/css-props.js
function css_props(element2, get_styles) {
  if (hydrating) {
    set_hydrate_node(get_first_child(element2));
  }
  render_effect(() => {
    var styles = get_styles();
    for (var key2 in styles) {
      var value = styles[key2];
      if (value) {
        element2.style.setProperty(key2, value);
      } else {
        element2.style.removeProperty(key2);
      }
    }
  });
}

// node_modules/svelte/src/internal/client/dom/blocks/each.js
function index(_, i) {
  return i;
}
function pause_effects(state2, to_destroy, controlled_anchor) {
  var transitions = [];
  var length = to_destroy.length;
  var group;
  var remaining = to_destroy.length;
  for (var i = 0; i < length; i++) {
    let effect2 = to_destroy[i];
    pause_effect(
      effect2,
      () => {
        if (group) {
          group.pending.delete(effect2);
          group.done.add(effect2);
          if (group.pending.size === 0) {
            var groups = (
              /** @type {Set<EachOutroGroup>} */
              state2.outrogroups
            );
            destroy_effects(state2, array_from(group.done));
            groups.delete(group);
            if (groups.size === 0) {
              state2.outrogroups = null;
            }
          }
        } else {
          remaining -= 1;
        }
      },
      false
    );
  }
  if (remaining === 0) {
    var fast_path = transitions.length === 0 && controlled_anchor !== null;
    if (fast_path) {
      var anchor = (
        /** @type {Element} */
        controlled_anchor
      );
      var parent_node = (
        /** @type {Element} */
        anchor.parentNode
      );
      clear_text_content(parent_node);
      parent_node.append(anchor);
      state2.items.clear();
    }
    destroy_effects(state2, to_destroy, !fast_path);
  } else {
    group = {
      pending: new Set(to_destroy),
      done: /* @__PURE__ */ new Set()
    };
    (state2.outrogroups ?? (state2.outrogroups = /* @__PURE__ */ new Set())).add(group);
  }
}
function destroy_effects(state2, to_destroy, remove_dom = true) {
  var preserved_effects;
  if (state2.pending.size > 0) {
    preserved_effects = /* @__PURE__ */ new Set();
    for (const keys of state2.pending.values()) {
      for (const key2 of keys) {
        preserved_effects.add(
          /** @type {EachItem} */
          state2.items.get(key2).e
        );
      }
    }
  }
  for (var i = 0; i < to_destroy.length; i++) {
    var e = to_destroy[i];
    if (preserved_effects == null ? void 0 : preserved_effects.has(e)) {
      e.f |= EFFECT_OFFSCREEN;
      const fragment = document.createDocumentFragment();
      move_effect(e, fragment);
    } else {
      destroy_effect(to_destroy[i], remove_dom);
    }
  }
}
var offscreen_anchor;
function each(node, flags, get_collection, get_key, render_fn, fallback_fn = null) {
  var anchor = node;
  var items = /* @__PURE__ */ new Map();
  var is_controlled = (flags & EACH_IS_CONTROLLED) !== 0;
  if (is_controlled) {
    var parent_node = (
      /** @type {Element} */
      node
    );
    anchor = hydrating ? set_hydrate_node(get_first_child(parent_node)) : parent_node.appendChild(create_text());
  }
  if (hydrating) {
    hydrate_next();
  }
  var fallback2 = null;
  var each_array = derived_safe_equal(() => {
    var collection = get_collection();
    return is_array(collection) ? collection : collection == null ? [] : array_from(collection);
  });
  if (true_default) {
    tag(each_array, "{#each ...}");
  }
  var array;
  var pending3 = /* @__PURE__ */ new Map();
  var first_run = true;
  function commit(batch) {
    if ((state2.effect.f & DESTROYED) !== 0) {
      return;
    }
    state2.pending.delete(batch);
    state2.fallback = fallback2;
    reconcile(state2, array, anchor, flags, get_key);
    if (fallback2 !== null) {
      if (array.length === 0) {
        if ((fallback2.f & EFFECT_OFFSCREEN) === 0) {
          resume_effect(fallback2);
        } else {
          fallback2.f ^= EFFECT_OFFSCREEN;
          move(fallback2, null, anchor);
        }
      } else {
        pause_effect(fallback2, () => {
          fallback2 = null;
        });
      }
    }
  }
  function discard(batch) {
    state2.pending.delete(batch);
  }
  var effect2 = block(() => {
    array = /** @type {V[]} */
    get(each_array);
    var length = array.length;
    let mismatch = false;
    if (hydrating) {
      var is_else = read_hydration_instruction(anchor) === HYDRATION_START_ELSE;
      if (is_else !== (length === 0)) {
        anchor = skip_nodes();
        set_hydrate_node(anchor);
        set_hydrating(false);
        mismatch = true;
      }
    }
    var keys = /* @__PURE__ */ new Set();
    var batch = (
      /** @type {Batch} */
      current_batch
    );
    var defer = should_defer_append();
    for (var index2 = 0; index2 < length; index2 += 1) {
      if (hydrating && hydrate_node.nodeType === COMMENT_NODE && /** @type {Comment} */
      hydrate_node.data === HYDRATION_END) {
        anchor = /** @type {Comment} */
        hydrate_node;
        mismatch = true;
        set_hydrating(false);
      }
      var value = array[index2];
      var key2 = get_key(value, index2);
      if (true_default) {
        var key_again = get_key(value, index2);
        if (key2 !== key_again) {
          each_key_volatile(String(index2), String(key2), String(key_again));
        }
      }
      var item = first_run ? null : items.get(key2);
      if (item) {
        if (item.v) internal_set(item.v, value);
        if (item.i) internal_set(item.i, index2);
        if (defer) {
          batch.unskip_effect(item.e);
        }
      } else {
        item = create_item(
          items,
          first_run ? anchor : offscreen_anchor ?? (offscreen_anchor = create_text()),
          value,
          key2,
          index2,
          render_fn,
          flags,
          get_collection
        );
        if (!first_run) {
          item.e.f |= EFFECT_OFFSCREEN;
        }
        items.set(key2, item);
      }
      keys.add(key2);
    }
    if (length === 0 && fallback_fn && !fallback2) {
      if (first_run) {
        fallback2 = branch(() => fallback_fn(anchor));
      } else {
        fallback2 = branch(() => fallback_fn(offscreen_anchor ?? (offscreen_anchor = create_text())));
        fallback2.f |= EFFECT_OFFSCREEN;
      }
    }
    if (length > keys.size) {
      if (true_default) {
        validate_each_keys(array, get_key);
      } else {
        each_key_duplicate("", "", "");
      }
    }
    if (hydrating && length > 0) {
      set_hydrate_node(skip_nodes());
    }
    if (!first_run) {
      pending3.set(batch, keys);
      if (defer) {
        for (const [key3, item2] of items) {
          if (!keys.has(key3)) {
            batch.skip_effect(item2.e);
          }
        }
        batch.oncommit(commit);
        batch.ondiscard(discard);
      } else {
        commit(batch);
      }
    }
    if (mismatch) {
      set_hydrating(true);
    }
    get(each_array);
  });
  var state2 = { effect: effect2, flags, items, pending: pending3, outrogroups: null, fallback: fallback2 };
  first_run = false;
  if (hydrating) {
    anchor = hydrate_node;
  }
}
function skip_to_branch(effect2) {
  while (effect2 !== null && (effect2.f & BRANCH_EFFECT) === 0) {
    effect2 = effect2.next;
  }
  return effect2;
}
function reconcile(state2, array, anchor, flags, get_key) {
  var _a, _b, _c, _d, _e, _f, _g, _h, _i;
  var is_animated = (flags & EACH_IS_ANIMATED) !== 0;
  var length = array.length;
  var items = state2.items;
  var current = skip_to_branch(state2.effect.first);
  var seen;
  var prev = null;
  var to_animate;
  var matched = [];
  var stashed = [];
  var value;
  var key2;
  var effect2;
  var i;
  if (is_animated) {
    for (i = 0; i < length; i += 1) {
      value = array[i];
      key2 = get_key(value, i);
      effect2 = /** @type {EachItem} */
      items.get(key2).e;
      if ((effect2.f & EFFECT_OFFSCREEN) === 0) {
        (_b = (_a = effect2.nodes) == null ? void 0 : _a.a) == null ? void 0 : _b.measure();
        (to_animate ?? (to_animate = /* @__PURE__ */ new Set())).add(effect2);
      }
    }
  }
  for (i = 0; i < length; i += 1) {
    value = array[i];
    key2 = get_key(value, i);
    effect2 = /** @type {EachItem} */
    items.get(key2).e;
    if (state2.outrogroups !== null) {
      for (const group of state2.outrogroups) {
        group.pending.delete(effect2);
        group.done.delete(effect2);
      }
    }
    if ((effect2.f & INERT) !== 0) {
      resume_effect(effect2);
      if (is_animated) {
        (_d = (_c = effect2.nodes) == null ? void 0 : _c.a) == null ? void 0 : _d.unfix();
        (to_animate ?? (to_animate = /* @__PURE__ */ new Set())).delete(effect2);
      }
    }
    if ((effect2.f & EFFECT_OFFSCREEN) !== 0) {
      effect2.f ^= EFFECT_OFFSCREEN;
      if (effect2 === current) {
        move(effect2, null, anchor);
      } else {
        var next2 = prev ? prev.next : current;
        if (effect2 === state2.effect.last) {
          state2.effect.last = effect2.prev;
        }
        if (effect2.prev) effect2.prev.next = effect2.next;
        if (effect2.next) effect2.next.prev = effect2.prev;
        link(state2, prev, effect2);
        link(state2, effect2, next2);
        move(effect2, next2, anchor);
        prev = effect2;
        matched = [];
        stashed = [];
        current = skip_to_branch(prev.next);
        continue;
      }
    }
    if (effect2 !== current) {
      if (seen !== void 0 && seen.has(effect2)) {
        if (matched.length < stashed.length) {
          var start = stashed[0];
          var j;
          prev = start.prev;
          var a = matched[0];
          var b = matched[matched.length - 1];
          for (j = 0; j < matched.length; j += 1) {
            move(matched[j], start, anchor);
          }
          for (j = 0; j < stashed.length; j += 1) {
            seen.delete(stashed[j]);
          }
          link(state2, a.prev, b.next);
          link(state2, prev, a);
          link(state2, b, start);
          current = start;
          prev = b;
          i -= 1;
          matched = [];
          stashed = [];
        } else {
          seen.delete(effect2);
          move(effect2, current, anchor);
          link(state2, effect2.prev, effect2.next);
          link(state2, effect2, prev === null ? state2.effect.first : prev.next);
          link(state2, prev, effect2);
          prev = effect2;
        }
        continue;
      }
      matched = [];
      stashed = [];
      while (current !== null && current !== effect2) {
        (seen ?? (seen = /* @__PURE__ */ new Set())).add(current);
        stashed.push(current);
        current = skip_to_branch(current.next);
      }
      if (current === null) {
        continue;
      }
    }
    if ((effect2.f & EFFECT_OFFSCREEN) === 0) {
      matched.push(effect2);
    }
    prev = effect2;
    current = skip_to_branch(effect2.next);
  }
  if (state2.outrogroups !== null) {
    for (const group of state2.outrogroups) {
      if (group.pending.size === 0) {
        destroy_effects(state2, array_from(group.done));
        (_e = state2.outrogroups) == null ? void 0 : _e.delete(group);
      }
    }
    if (state2.outrogroups.size === 0) {
      state2.outrogroups = null;
    }
  }
  if (current !== null || seen !== void 0) {
    var to_destroy = [];
    if (seen !== void 0) {
      for (effect2 of seen) {
        if ((effect2.f & INERT) === 0) {
          to_destroy.push(effect2);
        }
      }
    }
    while (current !== null) {
      if ((current.f & INERT) === 0 && current !== state2.fallback) {
        to_destroy.push(current);
      }
      current = skip_to_branch(current.next);
    }
    var destroy_length = to_destroy.length;
    if (destroy_length > 0) {
      var controlled_anchor = (flags & EACH_IS_CONTROLLED) !== 0 && length === 0 ? anchor : null;
      if (is_animated) {
        for (i = 0; i < destroy_length; i += 1) {
          (_g = (_f = to_destroy[i].nodes) == null ? void 0 : _f.a) == null ? void 0 : _g.measure();
        }
        for (i = 0; i < destroy_length; i += 1) {
          (_i = (_h = to_destroy[i].nodes) == null ? void 0 : _h.a) == null ? void 0 : _i.fix();
        }
      }
      pause_effects(state2, to_destroy, controlled_anchor);
    }
  }
  if (is_animated) {
    queue_micro_task(() => {
      var _a2, _b2;
      if (to_animate === void 0) return;
      for (effect2 of to_animate) {
        (_b2 = (_a2 = effect2.nodes) == null ? void 0 : _a2.a) == null ? void 0 : _b2.apply();
      }
    });
  }
}
function create_item(items, anchor, value, key2, index2, render_fn, flags, get_collection) {
  var v = (flags & EACH_ITEM_REACTIVE) !== 0 ? (flags & EACH_ITEM_IMMUTABLE) === 0 ? mutable_source(value, false, false) : source(value) : null;
  var i = (flags & EACH_INDEX_REACTIVE) !== 0 ? source(index2) : null;
  if (true_default && v) {
    v.trace = () => {
      get_collection()[(i == null ? void 0 : i.v) ?? index2];
    };
  }
  return {
    v,
    i,
    e: branch(() => {
      render_fn(anchor, v ?? value, i ?? index2, get_collection);
      return () => {
        items.delete(key2);
      };
    })
  };
}
function move(effect2, next2, anchor) {
  if (!effect2.nodes) return;
  var node = effect2.nodes.start;
  var end = effect2.nodes.end;
  var dest = next2 && (next2.f & EFFECT_OFFSCREEN) === 0 ? (
    /** @type {EffectNodes} */
    next2.nodes.start
  ) : anchor;
  while (node !== null) {
    var next_node = (
      /** @type {TemplateNode} */
      get_next_sibling(node)
    );
    dest.before(node);
    if (node === end) {
      return;
    }
    node = next_node;
  }
}
function link(state2, prev, next2) {
  if (prev === null) {
    state2.effect.first = next2;
  } else {
    prev.next = next2;
  }
  if (next2 === null) {
    state2.effect.last = prev;
  } else {
    next2.prev = prev;
  }
}
function validate_each_keys(array, key_fn) {
  const keys = /* @__PURE__ */ new Map();
  const length = array.length;
  for (let i = 0; i < length; i++) {
    const key2 = key_fn(array[i], i);
    if (keys.has(key2)) {
      const a = String(keys.get(key2));
      const b = String(i);
      let k = String(key2);
      if (k.startsWith("[object ")) k = null;
      each_key_duplicate(a, b, k);
    }
    keys.set(key2, i);
  }
}

// node_modules/svelte/src/internal/client/dom/blocks/html.js
function check_hash(element2, server_hash, value) {
  var _a, _b;
  if (!server_hash || server_hash === hash(String(value ?? ""))) return;
  let location;
  const loc = (_a = element2.__svelte_meta) == null ? void 0 : _a.loc;
  if (loc) {
    location = `near ${loc.file}:${loc.line}:${loc.column}`;
  } else if ((_b = dev_current_component_function) == null ? void 0 : _b[FILENAME]) {
    location = `in ${dev_current_component_function[FILENAME]}`;
  }
  hydration_html_changed(sanitize_location(location));
}
function html(node, get_value, is_controlled = false, svg = false, mathml = false, skip_warning = false) {
  var anchor = node;
  var value = "";
  if (is_controlled) {
    var parent_node = (
      /** @type {Element} */
      node
    );
    if (hydrating) {
      anchor = set_hydrate_node(get_first_child(parent_node));
    }
  }
  template_effect(() => {
    var effect2 = (
      /** @type {Effect} */
      active_effect
    );
    if (value === (value = get_value() ?? "")) {
      if (hydrating) hydrate_next();
      return;
    }
    if (is_controlled && !hydrating) {
      effect2.nodes = null;
      parent_node.innerHTML = /** @type {string} */
      value;
      if (value !== "") {
        assign_nodes(
          /** @type {TemplateNode} */
          get_first_child(parent_node),
          /** @type {TemplateNode} */
          parent_node.lastChild
        );
      }
      return;
    }
    if (effect2.nodes !== null) {
      remove_effect_dom(
        effect2.nodes.start,
        /** @type {TemplateNode} */
        effect2.nodes.end
      );
      effect2.nodes = null;
    }
    if (value === "") return;
    if (hydrating) {
      var hash2 = (
        /** @type {Comment} */
        hydrate_node.data
      );
      var next2 = hydrate_next();
      var last = next2;
      while (next2 !== null && (next2.nodeType !== COMMENT_NODE || /** @type {Comment} */
      next2.data !== "")) {
        last = next2;
        next2 = get_next_sibling(next2);
      }
      if (next2 === null) {
        hydration_mismatch();
        throw HYDRATION_ERROR;
      }
      if (true_default && !skip_warning) {
        check_hash(
          /** @type {Element} */
          next2.parentNode,
          hash2,
          value
        );
      }
      assign_nodes(hydrate_node, last);
      anchor = set_hydrate_node(next2);
      return;
    }
    var ns = svg ? NAMESPACE_SVG : mathml ? NAMESPACE_MATHML : void 0;
    var wrapper = (
      /** @type {HTMLTemplateElement | SVGElement | MathMLElement} */
      create_element(svg ? "svg" : mathml ? "math" : "template", ns)
    );
    wrapper.innerHTML = /** @type {any} */
    value;
    var node2 = svg || mathml ? wrapper : (
      /** @type {HTMLTemplateElement} */
      wrapper.content
    );
    assign_nodes(
      /** @type {TemplateNode} */
      get_first_child(node2),
      /** @type {TemplateNode} */
      node2.lastChild
    );
    if (svg || mathml) {
      while (get_first_child(node2)) {
        anchor.before(
          /** @type {TemplateNode} */
          get_first_child(node2)
        );
      }
    } else {
      anchor.before(node2);
    }
  });
}

// node_modules/svelte/src/internal/client/dom/blocks/slot.js
function slot(anchor, $$props, name, slot_props, fallback_fn) {
  var _a;
  if (hydrating) {
    hydrate_next();
  }
  var slot_fn = (_a = $$props.$$slots) == null ? void 0 : _a[name];
  var is_interop = false;
  if (slot_fn === true) {
    slot_fn = $$props[name === "default" ? "children" : name];
    is_interop = true;
  }
  if (slot_fn === void 0) {
    if (fallback_fn !== null) {
      fallback_fn(anchor);
    }
  } else {
    slot_fn(anchor, is_interop ? () => slot_props : slot_props);
  }
}
function sanitize_slots(props) {
  const sanitized = {};
  if (props.children) sanitized.default = true;
  for (const key2 in props.$$slots) {
    sanitized[key2] = true;
  }
  return sanitized;
}

// node_modules/svelte/src/internal/shared/validate.js
function validate_void_dynamic_element(tag_fn) {
  const tag2 = tag_fn();
  if (tag2 && is_void(tag2)) {
    dynamic_void_element_content(tag2);
  }
}
function validate_dynamic_element_tag(tag_fn) {
  const tag2 = tag_fn();
  const is_string = typeof tag2 === "string";
  if (tag2 && !is_string) {
    svelte_element_invalid_this_value();
  }
}
function validate_store(store, name) {
  if (store != null && typeof store.subscribe !== "function") {
    store_invalid_shape(name);
  }
}
function prevent_snippet_stringification(fn) {
  fn.toString = () => {
    snippet_without_render_tag();
    return "";
  };
  return fn;
}

// node_modules/svelte/src/internal/client/dom/blocks/snippet.js
function snippet(node, get_snippet, ...args) {
  var branches = new BranchManager(node);
  block(() => {
    const snippet2 = get_snippet() ?? null;
    if (true_default && snippet2 == null) {
      invalid_snippet();
    }
    branches.ensure(snippet2, snippet2 && ((anchor) => snippet2(anchor, ...args)));
  }, EFFECT_TRANSPARENT);
}
function wrap_snippet(component2, fn) {
  const snippet2 = (node, ...args) => {
    var previous_component_function = dev_current_component_function;
    set_dev_current_component_function(component2);
    try {
      return fn(node, ...args);
    } finally {
      set_dev_current_component_function(previous_component_function);
    }
  };
  prevent_snippet_stringification(snippet2);
  return snippet2;
}
function createRawSnippet(fn) {
  return (anchor, ...params) => {
    var _a;
    var snippet2 = fn(...params);
    var element2;
    if (hydrating) {
      element2 = /** @type {Element} */
      hydrate_node;
      hydrate_next();
    } else {
      var html2 = snippet2.render().trim();
      var fragment = create_fragment_from_html(html2);
      element2 = /** @type {Element} */
      get_first_child(fragment);
      if (true_default && (get_next_sibling(element2) !== null || element2.nodeType !== ELEMENT_NODE)) {
        invalid_raw_snippet_render();
      }
      anchor.before(element2);
    }
    const result = (_a = snippet2.setup) == null ? void 0 : _a.call(snippet2, element2);
    assign_nodes(element2, element2);
    if (typeof result === "function") {
      teardown(result);
    }
  };
}

// node_modules/svelte/src/internal/client/dom/blocks/svelte-component.js
function component(node, get_component, render_fn) {
  var hydration_start_node;
  if (hydrating) {
    hydration_start_node = hydrate_node;
    hydrate_next();
  }
  var branches = new BranchManager(node);
  block(() => {
    var component2 = get_component() ?? null;
    if (hydrating) {
      var data = read_hydration_instruction(
        /** @type {TemplateNode} */
        hydration_start_node
      );
      var server_had_component = data === HYDRATION_START;
      var client_has_component = component2 !== null;
      if (server_had_component !== client_has_component) {
        var anchor = skip_nodes();
        set_hydrate_node(anchor);
        branches.anchor = anchor;
        set_hydrating(false);
        branches.ensure(component2, component2 && ((target) => render_fn(target, component2)));
        set_hydrating(true);
        return;
      }
    }
    branches.ensure(component2, component2 && ((target) => render_fn(target, component2)));
  }, EFFECT_TRANSPARENT);
}

// node_modules/svelte/src/internal/client/timing.js
var now = true_default ? () => performance.now() : () => Date.now();
var raf = {
  // don't access requestAnimationFrame eagerly outside method
  // this allows basic testing of user code without JSDOM
  // bunder will eval and remove ternary when the user's app is built
  tick: (
    /** @param {any} _ */
    (_) => (true_default ? requestAnimationFrame : noop)(_)
  ),
  now: () => now(),
  tasks: /* @__PURE__ */ new Set()
};

// node_modules/svelte/src/internal/client/loop.js
function run_tasks() {
  const now2 = raf.now();
  raf.tasks.forEach((task) => {
    if (!task.c(now2)) {
      raf.tasks.delete(task);
      task.f();
    }
  });
  if (raf.tasks.size !== 0) {
    raf.tick(run_tasks);
  }
}
function loop(callback) {
  let task;
  if (raf.tasks.size === 0) {
    raf.tick(run_tasks);
  }
  return {
    promise: new Promise((fulfill) => {
      raf.tasks.add(task = { c: callback, f: fulfill });
    }),
    abort() {
      raf.tasks.delete(task);
    }
  };
}

// node_modules/svelte/src/internal/client/dom/elements/transitions.js
function dispatch_event(element2, type) {
  without_reactive_context(() => {
    element2.dispatchEvent(new CustomEvent(type));
  });
}
function css_property_to_camelcase(style) {
  if (style === "float") return "cssFloat";
  if (style === "offset") return "cssOffset";
  if (style.startsWith("--")) return style;
  const parts = style.split("-");
  if (parts.length === 1) return parts[0];
  return parts[0] + parts.slice(1).map(
    /** @param {any} word */
    (word) => word[0].toUpperCase() + word.slice(1)
  ).join("");
}
function css_to_keyframe(css) {
  const keyframe = {};
  const parts = css.split(";");
  for (const part of parts) {
    const [property, value] = part.split(":");
    if (!property || value === void 0) break;
    const formatted_property = css_property_to_camelcase(property.trim());
    keyframe[formatted_property] = value.trim();
  }
  return keyframe;
}
var linear = (t) => t;
var animation_effect_override = null;
function set_animation_effect_override(v) {
  animation_effect_override = v;
}
function animation(element2, get_fn, get_params) {
  var effect2 = animation_effect_override ?? /** @type {Effect} */
  active_effect;
  var nodes = (
    /** @type {EffectNodes} */
    effect2.nodes
  );
  var from;
  var to;
  var animation2;
  var original_styles = null;
  nodes.a ?? (nodes.a = {
    element: element2,
    measure() {
      from = this.element.getBoundingClientRect();
    },
    apply() {
      animation2 == null ? void 0 : animation2.abort();
      to = this.element.getBoundingClientRect();
      if (from.left !== to.left || from.right !== to.right || from.top !== to.top || from.bottom !== to.bottom) {
        const options = get_fn()(this.element, { from, to }, get_params == null ? void 0 : get_params());
        animation2 = animate(
          this.element,
          options,
          void 0,
          1,
          () => {
          },
          () => {
            animation2 == null ? void 0 : animation2.abort();
            animation2 = void 0;
          }
        );
      }
    },
    fix() {
      if (element2.getAnimations().length) return;
      var { position, width, height } = getComputedStyle(element2);
      if (position !== "absolute" && position !== "fixed") {
        var style = (
          /** @type {HTMLElement | SVGElement} */
          element2.style
        );
        original_styles = {
          position: style.position,
          width: style.width,
          height: style.height,
          transform: style.transform
        };
        style.position = "absolute";
        style.width = width;
        style.height = height;
        var to2 = element2.getBoundingClientRect();
        if (from.left !== to2.left || from.top !== to2.top) {
          var transform = `translate(${from.left - to2.left}px, ${from.top - to2.top}px)`;
          style.transform = style.transform ? `${style.transform} ${transform}` : transform;
        }
      }
    },
    unfix() {
      if (original_styles) {
        var style = (
          /** @type {HTMLElement | SVGElement} */
          element2.style
        );
        style.position = original_styles.position;
        style.width = original_styles.width;
        style.height = original_styles.height;
        style.transform = original_styles.transform;
      }
    }
  });
  nodes.a.element = element2;
}
function transition(flags, element2, get_fn, get_params) {
  var _a;
  var is_intro = (flags & TRANSITION_IN) !== 0;
  var is_outro = (flags & TRANSITION_OUT) !== 0;
  var is_both = is_intro && is_outro;
  var is_global = (flags & TRANSITION_GLOBAL) !== 0;
  var direction = is_both ? "both" : is_intro ? "in" : "out";
  var current_options;
  var inert = element2.inert;
  var overflow = element2.style.overflow;
  var intro;
  var outro;
  function get_options() {
    return without_reactive_context(() => {
      return current_options ?? (current_options = get_fn()(element2, (get_params == null ? void 0 : get_params()) ?? /** @type {P} */
      {}, {
        direction
      }));
    });
  }
  var transition2 = {
    is_global,
    in() {
      var _a2;
      element2.inert = inert;
      if (!is_intro) {
        outro == null ? void 0 : outro.abort();
        (_a2 = outro == null ? void 0 : outro.reset) == null ? void 0 : _a2.call(outro);
        return;
      }
      if (!is_outro) {
        intro == null ? void 0 : intro.abort();
      }
      intro = animate(
        element2,
        get_options(),
        outro,
        1,
        () => {
          dispatch_event(element2, "introstart");
        },
        () => {
          dispatch_event(element2, "introend");
          intro == null ? void 0 : intro.abort();
          intro = current_options = void 0;
          element2.style.overflow = overflow;
        }
      );
    },
    out(fn) {
      if (!is_outro) {
        fn == null ? void 0 : fn();
        current_options = void 0;
        return;
      }
      element2.inert = true;
      outro = animate(
        element2,
        get_options(),
        intro,
        0,
        () => {
          dispatch_event(element2, "outrostart");
        },
        () => {
          dispatch_event(element2, "outroend");
          fn == null ? void 0 : fn();
        }
      );
    },
    stop: () => {
      intro == null ? void 0 : intro.abort();
      outro == null ? void 0 : outro.abort();
    }
  };
  var e = (
    /** @type {Effect & { nodes: EffectNodes }} */
    active_effect
  );
  ((_a = e.nodes).t ?? (_a.t = [])).push(transition2);
  if (is_intro && should_intro) {
    var run3 = is_global;
    if (!run3) {
      var block2 = (
        /** @type {Effect | null} */
        e.parent
      );
      while (block2 && (block2.f & EFFECT_TRANSPARENT) !== 0) {
        while (block2 = block2.parent) {
          if ((block2.f & BLOCK_EFFECT) !== 0) break;
        }
      }
      run3 = !block2 || (block2.f & REACTION_RAN) !== 0;
    }
    if (run3) {
      effect(() => {
        untrack(() => transition2.in());
      });
    }
  }
}
function animate(element2, options, counterpart, t2, on_begin, on_finish) {
  var is_intro = t2 === 1;
  if (is_function(options)) {
    var a;
    var aborted2 = false;
    queue_micro_task(() => {
      if (aborted2) return;
      var o = options({ direction: is_intro ? "in" : "out" });
      a = animate(element2, o, counterpart, t2, on_begin, on_finish);
    });
    return {
      abort: () => {
        aborted2 = true;
        a == null ? void 0 : a.abort();
      },
      deactivate: () => a.deactivate(),
      reset: () => a.reset(),
      t: () => a.t()
    };
  }
  counterpart == null ? void 0 : counterpart.deactivate();
  if (!(options == null ? void 0 : options.duration) && !(options == null ? void 0 : options.delay)) {
    on_begin();
    on_finish();
    return {
      abort: noop,
      deactivate: noop,
      reset: noop,
      t: () => t2
    };
  }
  const { delay = 0, css, tick: tick2, easing = linear } = options;
  var keyframes = [];
  if (is_intro && counterpart === void 0) {
    if (tick2) {
      tick2(0, 1);
    }
    if (css) {
      var styles = css_to_keyframe(css(0, 1));
      keyframes.push(styles, styles);
    }
  }
  var get_t = () => 1 - t2;
  var animation2 = element2.animate(keyframes, { duration: delay, fill: "forwards" });
  animation2.onfinish = () => {
    animation2.cancel();
    on_begin();
    var t1 = (counterpart == null ? void 0 : counterpart.t()) ?? 1 - t2;
    counterpart == null ? void 0 : counterpart.abort();
    var delta = t2 - t1;
    var duration = (
      /** @type {number} */
      options.duration * Math.abs(delta)
    );
    var keyframes2 = [];
    if (duration > 0) {
      var needs_overflow_hidden = false;
      if (css) {
        var n = Math.ceil(duration / (1e3 / 60));
        for (var i = 0; i <= n; i += 1) {
          var t = t1 + delta * easing(i / n);
          var styles2 = css_to_keyframe(css(t, 1 - t));
          keyframes2.push(styles2);
          needs_overflow_hidden || (needs_overflow_hidden = styles2.overflow === "hidden");
        }
      }
      if (needs_overflow_hidden) {
        element2.style.overflow = "hidden";
      }
      get_t = () => {
        var time = (
          /** @type {number} */
          /** @type {globalThis.Animation} */
          animation2.currentTime
        );
        return t1 + delta * easing(time / duration);
      };
      if (tick2) {
        loop(() => {
          if (animation2.playState !== "running") return false;
          var t3 = get_t();
          tick2(t3, 1 - t3);
          return true;
        });
      }
    }
    animation2 = element2.animate(keyframes2, { duration, fill: "forwards" });
    animation2.onfinish = () => {
      get_t = () => t2;
      tick2 == null ? void 0 : tick2(t2, 1 - t2);
      on_finish();
    };
  };
  return {
    abort: () => {
      if (animation2) {
        animation2.cancel();
        animation2.effect = null;
        animation2.onfinish = noop;
      }
    },
    deactivate: () => {
      on_finish = noop;
    },
    reset: () => {
      if (t2 === 0) {
        tick2 == null ? void 0 : tick2(1, 0);
      }
    },
    t: () => get_t()
  };
}

// node_modules/svelte/src/internal/client/dom/blocks/svelte-element.js
function element(node, get_tag, is_svg, render_fn, get_namespace, location) {
  var _a;
  let was_hydrating = hydrating;
  if (hydrating) {
    hydrate_next();
  }
  var filename = true_default && location && ((_a = component_context) == null ? void 0 : _a.function[FILENAME]);
  var element2 = null;
  if (hydrating && hydrate_node.nodeType === ELEMENT_NODE) {
    element2 = /** @type {Element} */
    hydrate_node;
    hydrate_next();
  }
  var anchor = (
    /** @type {TemplateNode} */
    hydrating ? hydrate_node : node
  );
  var parent_effect = (
    /** @type {Effect} */
    active_effect
  );
  var branches = new BranchManager(anchor, false);
  block(() => {
    const next_tag = get_tag() || null;
    var ns = get_namespace ? get_namespace() : is_svg || next_tag === "svg" ? NAMESPACE_SVG : void 0;
    if (next_tag === null) {
      branches.ensure(null, null);
      set_should_intro(true);
      return;
    }
    branches.ensure(next_tag, (anchor2) => {
      if (next_tag) {
        element2 = hydrating ? (
          /** @type {Element} */
          element2
        ) : create_element(next_tag, ns);
        if (true_default && location) {
          element2.__svelte_meta = {
            parent: dev_stack,
            loc: {
              file: filename,
              line: location[0],
              column: location[1]
            }
          };
        }
        assign_nodes(element2, element2);
        if (render_fn) {
          if (hydrating && is_raw_text_element(next_tag)) {
            element2.append(document.createComment(""));
          }
          var child_anchor = hydrating ? get_first_child(element2) : element2.appendChild(create_text());
          if (hydrating) {
            if (child_anchor === null) {
              set_hydrating(false);
            } else {
              set_hydrate_node(child_anchor);
            }
          }
          set_animation_effect_override(parent_effect);
          render_fn(element2, child_anchor);
          set_animation_effect_override(null);
        }
        active_effect.nodes.end = element2;
        anchor2.before(element2);
      }
      if (hydrating) {
        set_hydrate_node(anchor2);
      }
    });
    set_should_intro(true);
    return () => {
      if (next_tag) {
        set_should_intro(false);
      }
    };
  }, EFFECT_TRANSPARENT);
  teardown(() => {
    set_should_intro(true);
  });
  if (was_hydrating) {
    set_hydrating(true);
    set_hydrate_node(anchor);
  }
}

// node_modules/svelte/src/internal/client/dom/blocks/svelte-head.js
function head(hash2, render_fn) {
  let previous_hydrate_node = null;
  let was_hydrating = hydrating;
  var anchor;
  if (hydrating) {
    previous_hydrate_node = hydrate_node;
    var head_anchor = get_first_child(document.head);
    while (head_anchor !== null && (head_anchor.nodeType !== COMMENT_NODE || /** @type {Comment} */
    head_anchor.data !== hash2)) {
      head_anchor = get_next_sibling(head_anchor);
    }
    if (head_anchor === null) {
      set_hydrating(false);
    } else {
      var start = (
        /** @type {TemplateNode} */
        get_next_sibling(head_anchor)
      );
      head_anchor.remove();
      set_hydrate_node(start);
    }
  }
  if (!hydrating) {
    anchor = document.head.appendChild(create_text());
  }
  try {
    block(() => {
      var e = branch(() => render_fn(anchor));
      e.f |= HEAD_EFFECT;
    });
  } finally {
    if (was_hydrating) {
      set_hydrating(true);
      set_hydrate_node(
        /** @type {TemplateNode} */
        previous_hydrate_node
      );
    }
  }
}

// node_modules/svelte/src/internal/client/dom/css.js
function append_styles(anchor, css) {
  effect(() => {
    var root = anchor.getRootNode();
    var target = (
      /** @type {ShadowRoot} */
      root.host ? (
        /** @type {ShadowRoot} */
        root
      ) : (
        /** @type {Document} */
        root.head ?? /** @type {Document} */
        root.ownerDocument.head
      )
    );
    if (!target.querySelector("#" + css.hash)) {
      const style = create_element("style");
      style.id = css.hash;
      style.textContent = css.code;
      target.appendChild(style);
      if (true_default) {
        register_style(css.hash, style);
      }
    }
  });
}

// node_modules/svelte/src/internal/client/dom/elements/actions.js
function action(dom, action2, get_value) {
  effect(() => {
    var payload = untrack(() => action2(dom, get_value == null ? void 0 : get_value()) || {});
    if (get_value && (payload == null ? void 0 : payload.update)) {
      var inited = false;
      var prev = (
        /** @type {any} */
        {}
      );
      render_effect(() => {
        var value = get_value();
        deep_read_state(value);
        if (inited && safe_not_equal(prev, value)) {
          prev = value;
          payload.update(value);
        }
      });
      inited = true;
    }
    if (payload == null ? void 0 : payload.destroy) {
      return () => (
        /** @type {Function} */
        payload.destroy()
      );
    }
  });
}

// node_modules/svelte/src/internal/client/dom/elements/attachments.js
function attach(node, get_fn) {
  var fn = void 0;
  var e;
  managed(() => {
    if (fn !== (fn = get_fn())) {
      if (e) {
        destroy_effect(e);
        e = null;
      }
      if (fn) {
        e = branch(() => {
          effect(() => (
            /** @type {(node: Element) => void} */
            fn(node)
          ));
        });
      }
    }
  });
}

// node_modules/svelte/src/escaping.js
var ATTR_REGEX = /[&"<]/g;
var CONTENT_REGEX = /[&<]/g;
function escape_html(value, is_attr) {
  const str = String(value ?? "");
  const pattern = is_attr ? ATTR_REGEX : CONTENT_REGEX;
  pattern.lastIndex = 0;
  let escaped = "";
  let last = 0;
  while (pattern.test(str)) {
    const i = pattern.lastIndex - 1;
    const ch = str[i];
    escaped += str.substring(last, i) + (ch === "&" ? "&amp;" : ch === '"' ? "&quot;" : "&lt;");
    last = i + 1;
  }
  return escaped + str.substring(last);
}

// node_modules/svelte/src/internal/shared/attributes.js
var replacements = {
  translate: /* @__PURE__ */ new Map([
    [true, "yes"],
    [false, "no"]
  ])
};
function attr(name, value, is_boolean = false) {
  if (name === "hidden" && value !== "until-found") {
    is_boolean = true;
  }
  if (value == null || !value && is_boolean) return "";
  const normalized = has_own_property.call(replacements, name) && replacements[name].get(value) || value;
  const assignment = is_boolean ? `=""` : `="${escape_html(normalized, true)}"`;
  return ` ${name}${assignment}`;
}
function clsx2(value) {
  if (typeof value === "object") {
    return clsx(value);
  } else {
    return value ?? "";
  }
}
var whitespace = [..." 	\n\r\f \v\uFEFF"];
function to_class(value, hash2, directives) {
  var classname = value == null ? "" : "" + value;
  if (hash2) {
    classname = classname ? classname + " " + hash2 : hash2;
  }
  if (directives) {
    for (var key2 of Object.keys(directives)) {
      if (directives[key2]) {
        classname = classname ? classname + " " + key2 : key2;
      } else if (classname.length) {
        var len = key2.length;
        var a = 0;
        while ((a = classname.indexOf(key2, a)) >= 0) {
          var b = a + len;
          if ((a === 0 || whitespace.includes(classname[a - 1])) && (b === classname.length || whitespace.includes(classname[b]))) {
            classname = (a === 0 ? "" : classname.substring(0, a)) + classname.substring(b + 1);
          } else {
            a = b;
          }
        }
      }
    }
  }
  return classname === "" ? null : classname;
}
function append_styles2(styles, important = false) {
  var separator = important ? " !important;" : ";";
  var css = "";
  for (var key2 of Object.keys(styles)) {
    var value = styles[key2];
    if (value != null && value !== "") {
      css += " " + key2 + ": " + value + separator;
    }
  }
  return css;
}
function to_css_name(name) {
  if (name[0] !== "-" || name[1] !== "-") {
    return name.toLowerCase();
  }
  return name;
}
function to_style(value, styles) {
  if (styles) {
    var new_style = "";
    var normal_styles;
    var important_styles;
    if (Array.isArray(styles)) {
      normal_styles = styles[0];
      important_styles = styles[1];
    } else {
      normal_styles = styles;
    }
    if (value) {
      value = String(value).replaceAll(/\s*\/\*.*?\*\/\s*/g, "").trim();
      var in_str = false;
      var in_apo = 0;
      var in_comment = false;
      var reserved_names = [];
      if (normal_styles) {
        reserved_names.push(...Object.keys(normal_styles).map(to_css_name));
      }
      if (important_styles) {
        reserved_names.push(...Object.keys(important_styles).map(to_css_name));
      }
      var start_index = 0;
      var name_index = -1;
      const len = value.length;
      for (var i = 0; i < len; i++) {
        var c = value[i];
        if (in_comment) {
          if (c === "/" && value[i - 1] === "*") {
            in_comment = false;
          }
        } else if (in_str) {
          if (in_str === c) {
            in_str = false;
          }
        } else if (c === "/" && value[i + 1] === "*") {
          in_comment = true;
        } else if (c === '"' || c === "'") {
          in_str = c;
        } else if (c === "(") {
          in_apo++;
        } else if (c === ")") {
          in_apo--;
        }
        if (!in_comment && in_str === false && in_apo === 0) {
          if (c === ":" && name_index === -1) {
            name_index = i;
          } else if (c === ";" || i === len - 1) {
            if (name_index !== -1) {
              var name = to_css_name(value.substring(start_index, name_index).trim());
              if (!reserved_names.includes(name)) {
                if (c !== ";") {
                  i++;
                }
                var property = value.substring(start_index, i).trim();
                new_style += " " + property + ";";
              }
            }
            start_index = i + 1;
            name_index = -1;
          }
        }
      }
    }
    if (normal_styles) {
      new_style += append_styles2(normal_styles);
    }
    if (important_styles) {
      new_style += append_styles2(important_styles, true);
    }
    new_style = new_style.trim();
    return new_style === "" ? null : new_style;
  }
  return value == null ? null : String(value);
}

// node_modules/svelte/src/internal/client/dom/elements/class.js
function set_class(dom, is_html, value, hash2, prev_classes, next_classes) {
  var prev = (
    /** @type {any} */
    dom[CLASS_CACHE]
  );
  if (hydrating || prev !== value || prev === void 0) {
    var next_class_name = to_class(value, hash2, next_classes);
    if (!hydrating || next_class_name !== dom.getAttribute("class")) {
      if (next_class_name == null) {
        dom.removeAttribute("class");
      } else if (is_html) {
        dom.className = next_class_name;
      } else {
        dom.setAttribute("class", next_class_name);
      }
    }
    dom[CLASS_CACHE] = value;
  } else if (next_classes && prev_classes !== next_classes) {
    for (var key2 in next_classes) {
      var is_present = !!next_classes[key2];
      if (prev_classes == null || is_present !== !!prev_classes[key2]) {
        dom.classList.toggle(key2, is_present);
      }
    }
  }
  return next_classes;
}

// node_modules/svelte/src/internal/client/dom/elements/style.js
function update_styles(dom, prev = {}, next2, priority) {
  for (var key2 in next2) {
    var value = next2[key2];
    if (prev[key2] !== value) {
      if (next2[key2] == null) {
        dom.style.removeProperty(key2);
      } else {
        dom.style.setProperty(key2, value, priority);
      }
    }
  }
}
function set_style(dom, value, prev_styles, next_styles) {
  var prev = (
    /** @type {any} */
    dom[STYLE_CACHE]
  );
  if (hydrating || prev !== value) {
    var next_style_attr = to_style(value, next_styles);
    if (!hydrating || next_style_attr !== dom.getAttribute("style")) {
      if (next_style_attr == null) {
        dom.removeAttribute("style");
      } else {
        dom.style.cssText = next_style_attr;
      }
    }
    dom[STYLE_CACHE] = value;
  } else if (next_styles) {
    if (Array.isArray(next_styles)) {
      update_styles(dom, prev_styles == null ? void 0 : prev_styles[0], next_styles[0]);
      update_styles(dom, prev_styles == null ? void 0 : prev_styles[1], next_styles[1], "important");
    } else {
      update_styles(dom, prev_styles, next_styles);
    }
  }
  return next_styles;
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/select.js
function select_option(select, value, mounting = false) {
  if (select.multiple) {
    if (value == void 0) {
      return;
    }
    if (!is_array(value)) {
      return select_multiple_invalid_value();
    }
    for (var option of select.options) {
      option.selected = value.includes(get_option_value(option));
    }
    return;
  }
  for (option of select.options) {
    var option_value = get_option_value(option);
    if (is(option_value, value)) {
      option.selected = true;
      return;
    }
  }
  if (!mounting || value !== void 0) {
    select.selectedIndex = -1;
  }
}
function init_select(select) {
  var observer = new MutationObserver(() => {
    select_option(select, select.__value);
  });
  observer.observe(select, {
    // Listen to option element changes
    childList: true,
    subtree: true,
    // because of <optgroup>
    // Listen to option element value attribute changes
    // (doesn't get notified of select value changes,
    // because that property is not reflected as an attribute)
    attributes: true,
    attributeFilter: ["value"]
  });
  teardown(() => {
    observer.disconnect();
  });
}
function bind_select_value(select, get2, set2 = get2) {
  var batches = /* @__PURE__ */ new WeakSet();
  var mounting = true;
  listen_to_event_and_reset_event(select, "change", (is_reset) => {
    var query = is_reset ? "[selected]" : ":checked";
    var value;
    if (select.multiple) {
      value = [].map.call(select.querySelectorAll(query), get_option_value);
    } else {
      var selected_option = select.querySelector(query) ?? // will fall back to first non-disabled option if no option is selected
      select.querySelector("option:not([disabled])");
      value = selected_option && get_option_value(selected_option);
    }
    set2(value);
    select.__value = value;
    if (current_batch !== null) {
      batches.add(current_batch);
    }
  });
  effect(() => {
    var value = get2();
    if (select === document.activeElement) {
      var batch = (
        /** @type {Batch} */
        async_mode_flag ? previous_batch : current_batch
      );
      if (batches.has(batch)) {
        return;
      }
    }
    select_option(select, value, mounting);
    if (mounting && value === void 0) {
      var selected_option = select.querySelector(":checked");
      if (selected_option !== null) {
        value = get_option_value(selected_option);
        set2(value);
      }
    }
    select.__value = value;
    mounting = false;
  });
  init_select(select);
}
function get_option_value(option) {
  if ("__value" in option) {
    return option.__value;
  } else {
    return option.value;
  }
}

// node_modules/svelte/src/internal/client/dom/elements/attributes.js
var CLASS = Symbol("class");
var STYLE = Symbol("style");
var IS_CUSTOM_ELEMENT = Symbol("is custom element");
var IS_HTML = Symbol("is html");
var LINK_TAG = IS_XHTML ? "link" : "LINK";
var INPUT_TAG = IS_XHTML ? "input" : "INPUT";
var OPTION_TAG = IS_XHTML ? "option" : "OPTION";
var SELECT_TAG = IS_XHTML ? "select" : "SELECT";
var PROGRESS_TAG = IS_XHTML ? "progress" : "PROGRESS";
function remove_input_defaults(input) {
  if (!hydrating) return;
  var already_removed = false;
  var remove_defaults = () => {
    if (already_removed) return;
    already_removed = true;
    if (input.hasAttribute("value")) {
      var value = input.value;
      set_attribute(input, "value", null);
      input.value = value;
    }
    if (input.hasAttribute("checked")) {
      var checked = input.checked;
      set_attribute(input, "checked", null);
      input.checked = checked;
    }
  };
  input[FORM_RESET_HANDLER] = remove_defaults;
  queue_micro_task(remove_defaults);
  add_form_reset_listener();
}
function set_value(element2, value) {
  var attributes = get_attributes(element2);
  if (attributes.value === (attributes.value = // treat null and undefined the same for the initial value
  value ?? void 0) || // @ts-expect-error
  // `progress` elements always need their value set when it's `0`
  element2.value === value && (value !== 0 || element2.nodeName !== PROGRESS_TAG)) {
    return;
  }
  element2.value = value ?? "";
}
function set_checked(element2, checked) {
  var attributes = get_attributes(element2);
  if (attributes.checked === (attributes.checked = // treat null and undefined the same for the initial value
  checked ?? void 0)) {
    return;
  }
  element2.checked = checked;
}
function set_selected(element2, selected) {
  if (selected) {
    if (!element2.hasAttribute("selected")) {
      element2.setAttribute("selected", "");
    }
  } else {
    element2.removeAttribute("selected");
  }
}
function set_default_checked(element2, checked) {
  const existing_value = element2.checked;
  element2.defaultChecked = checked;
  element2.checked = existing_value;
}
function set_default_value(element2, value) {
  const existing_value = element2.value;
  element2.defaultValue = value;
  element2.value = existing_value;
}
function set_attribute(element2, attribute, value, skip_warning) {
  var attributes = get_attributes(element2);
  if (hydrating) {
    attributes[attribute] = element2.getAttribute(attribute);
    if (attribute === "src" || attribute === "srcset" || attribute === "href" && element2.nodeName === LINK_TAG) {
      if (!skip_warning) {
        check_src_in_dev_hydration(element2, attribute, value ?? "");
      }
      return;
    }
  }
  if (attributes[attribute] === (attributes[attribute] = value)) return;
  if (attribute === "loading") {
    element2[LOADING_ATTR_SYMBOL] = value;
  }
  if (value == null) {
    element2.removeAttribute(attribute);
  } else if (typeof value !== "string" && get_setters(element2).includes(attribute)) {
    element2[attribute] = value;
  } else {
    element2.setAttribute(attribute, value);
  }
}
function set_xlink_attribute(dom, attribute, value) {
  dom.setAttributeNS("http://www.w3.org/1999/xlink", attribute, value);
}
function set_custom_element_data(node, prop2, value) {
  var previous_reaction = active_reaction;
  var previous_effect = active_effect;
  let was_hydrating = hydrating;
  if (hydrating) {
    set_hydrating(false);
  }
  set_active_reaction(null);
  set_active_effect(null);
  try {
    if (
      // `style` should use `set_attribute` rather than the setter
      prop2 !== "style" && // Don't compute setters for custom elements while they aren't registered yet,
      // because during their upgrade/instantiation they might add more setters.
      // Instead, fall back to a simple "an object, then set as property" heuristic.
      (setters_cache.has(node.getAttribute("is") || node.nodeName) || // customElements may not be available in browser extension contexts
      !customElements || customElements.get(node.getAttribute("is") || node.nodeName.toLowerCase()) ? get_setters(node).includes(prop2) : value && typeof value === "object")
    ) {
      node[prop2] = value;
    } else {
      set_attribute(node, prop2, value == null ? value : String(value));
    }
  } finally {
    set_active_reaction(previous_reaction);
    set_active_effect(previous_effect);
    if (was_hydrating) {
      set_hydrating(true);
    }
  }
}
function set_attributes(element2, prev, next2, css_hash, should_remove_defaults = false, skip_warning = false) {
  if (hydrating && should_remove_defaults && element2.nodeName === INPUT_TAG) {
    var input = (
      /** @type {HTMLInputElement} */
      element2
    );
    var attribute = input.type === "checkbox" ? "defaultChecked" : "defaultValue";
    if (!(attribute in next2)) {
      remove_input_defaults(input);
    }
  }
  var attributes = get_attributes(element2);
  var is_custom_element = attributes[IS_CUSTOM_ELEMENT];
  var preserve_attribute_case = !attributes[IS_HTML];
  let is_hydrating_custom_element = hydrating && is_custom_element;
  if (is_hydrating_custom_element) {
    set_hydrating(false);
  }
  var current = prev || {};
  var is_option_element = element2.nodeName === OPTION_TAG;
  for (var key2 in prev) {
    if (!(key2 in next2)) {
      next2[key2] = null;
    }
  }
  if (next2.class) {
    next2.class = clsx2(next2.class);
  } else if (css_hash || next2[CLASS]) {
    next2.class = null;
  }
  if (next2[STYLE]) {
    next2.style ?? (next2.style = null);
  }
  var setters = get_setters(element2);
  for (const key3 in next2) {
    let value = next2[key3];
    if (is_option_element && key3 === "value" && value == null) {
      element2.value = element2.__value = "";
      current[key3] = value;
      continue;
    }
    if (key3 === "class") {
      var is_html = element2.namespaceURI === "http://www.w3.org/1999/xhtml";
      set_class(element2, is_html, value, css_hash, prev == null ? void 0 : prev[CLASS], next2[CLASS]);
      current[key3] = value;
      current[CLASS] = next2[CLASS];
      continue;
    }
    if (key3 === "style") {
      set_style(element2, value, prev == null ? void 0 : prev[STYLE], next2[STYLE]);
      current[key3] = value;
      current[STYLE] = next2[STYLE];
      continue;
    }
    var prev_value = current[key3];
    if (value === prev_value && !(value === void 0 && element2.hasAttribute(key3))) {
      continue;
    }
    current[key3] = value;
    var prefix = key3[0] + key3[1];
    if (prefix === "$$") continue;
    if (prefix === "on") {
      const opts = {};
      const event_handle_key = "$$" + key3;
      let event_name = key3.slice(2);
      var is_delegated = can_delegate_event(event_name);
      if (is_capture_event(event_name)) {
        event_name = event_name.slice(0, -7);
        opts.capture = true;
      }
      if (!is_delegated && prev_value) {
        if (value != null) continue;
        element2.removeEventListener(event_name, current[event_handle_key], opts);
        current[event_handle_key] = null;
      }
      if (is_delegated) {
        delegated(event_name, element2, value);
        delegate([event_name]);
      } else if (value != null) {
        let handle = function(evt) {
          current[key3].call(this, evt);
        };
        current[event_handle_key] = create_event(event_name, element2, handle, opts);
      }
    } else if (key3 === "style") {
      set_attribute(element2, key3, value);
    } else if (key3 === "autofocus") {
      autofocus(
        /** @type {HTMLElement} */
        element2,
        Boolean(value)
      );
    } else if (!is_custom_element && (key3 === "__value" || key3 === "value" && value != null)) {
      element2.value = element2.__value = value;
    } else if (key3 === "selected" && is_option_element) {
      set_selected(
        /** @type {HTMLOptionElement} */
        element2,
        value
      );
    } else {
      var name = key3;
      if (!preserve_attribute_case) {
        name = normalize_attribute(name);
      }
      var is_default = name === "defaultValue" || name === "defaultChecked";
      if (value == null && !is_custom_element && !is_default) {
        attributes[key3] = null;
        if (name === "value" || name === "checked") {
          let input2 = (
            /** @type {HTMLInputElement} */
            element2
          );
          const use_default = prev === void 0;
          if (name === "value") {
            let previous = input2.defaultValue;
            input2.removeAttribute(name);
            input2.defaultValue = previous;
            input2.value = input2.__value = use_default ? previous : null;
          } else {
            let previous = input2.defaultChecked;
            input2.removeAttribute(name);
            input2.defaultChecked = previous;
            input2.checked = use_default ? previous : false;
          }
        } else {
          element2.removeAttribute(key3);
        }
      } else if (is_default || setters.includes(name) && (is_custom_element || typeof value !== "string")) {
        element2[name] = value;
        if (name in attributes) attributes[name] = UNINITIALIZED;
      } else if (typeof value !== "function") {
        set_attribute(element2, name, value, skip_warning);
      }
    }
  }
  if (is_hydrating_custom_element) {
    set_hydrating(true);
  }
  return current;
}
function attribute_effect(element2, fn, sync = [], async2 = [], blockers = [], css_hash, should_remove_defaults = false, skip_warning = false) {
  flatten(blockers, sync, async2, (values) => {
    var prev = void 0;
    var effects = {};
    var is_select = element2.nodeName === SELECT_TAG;
    var inited = false;
    managed(() => {
      var next2 = fn(...values.map(get));
      var current = set_attributes(
        element2,
        prev,
        next2,
        css_hash,
        should_remove_defaults,
        skip_warning
      );
      if (inited && is_select && "value" in next2) {
        select_option(
          /** @type {HTMLSelectElement} */
          element2,
          next2.value
        );
      }
      for (let symbol of Object.getOwnPropertySymbols(effects)) {
        if (!next2[symbol]) destroy_effect(effects[symbol]);
      }
      for (let symbol of Object.getOwnPropertySymbols(next2)) {
        var n = next2[symbol];
        if (symbol.description === ATTACHMENT_KEY && (!prev || n !== prev[symbol])) {
          if (effects[symbol]) destroy_effect(effects[symbol]);
          effects[symbol] = branch(() => attach(element2, () => n));
        }
        current[symbol] = n;
      }
      prev = current;
    });
    if (is_select) {
      var select = (
        /** @type {HTMLSelectElement} */
        element2
      );
      effect(() => {
        select_option(
          select,
          /** @type {Record<string | symbol, any>} */
          prev.value,
          true
        );
        init_select(select);
      });
    }
    inited = true;
  });
}
function get_attributes(element2) {
  var _a;
  return (
    /** @type {Record<string | symbol, unknown>} **/
    /** @type {any} */
    element2[_a = ATTRIBUTES_CACHE] ?? (element2[_a] = {
      [IS_CUSTOM_ELEMENT]: element2.nodeName.includes("-"),
      [IS_HTML]: element2.namespaceURI === NAMESPACE_HTML
    })
  );
}
var setters_cache = /* @__PURE__ */ new Map();
function get_setters(element2) {
  var cache_key = element2.getAttribute("is") || element2.nodeName;
  var setters = setters_cache.get(cache_key);
  if (setters) return setters;
  setters_cache.set(cache_key, setters = []);
  var descriptors;
  var proto = element2;
  var element_proto = Element.prototype;
  while (element_proto !== proto) {
    descriptors = get_descriptors(proto);
    for (var key2 in descriptors) {
      if (descriptors[key2].set && // better safe than sorry, we don't want spread attributes to mess with HTML content
      key2 !== "innerHTML" && key2 !== "textContent" && key2 !== "innerText") {
        setters.push(key2);
      }
    }
    proto = get_prototype_of(proto);
  }
  return setters;
}
function check_src_in_dev_hydration(element2, attribute, value) {
  if (!true_default) return;
  if (attribute === "srcset" && srcset_url_equal(element2, value)) return;
  if (src_url_equal(element2.getAttribute(attribute) ?? "", value)) return;
  hydration_attribute_changed(
    attribute,
    element2.outerHTML.replace(element2.innerHTML, element2.innerHTML && "..."),
    String(value)
  );
}
function src_url_equal(element_src, url) {
  if (element_src === url) return true;
  return new URL(element_src, document.baseURI).href === new URL(url, document.baseURI).href;
}
function split_srcset(srcset) {
  return srcset.split(",").map((src) => src.trim().split(" ").filter(Boolean));
}
function srcset_url_equal(element2, srcset) {
  var element_urls = split_srcset(element2.srcset);
  var urls = split_srcset(srcset);
  return urls.length === element_urls.length && urls.every(
    ([url, width], i) => width === element_urls[i][1] && // We need to test both ways because Vite will create an a full URL with
    // `new URL(asset, import.meta.url).href` for the client when `base: './'`, and the
    // relative URLs inside srcset are not automatically resolved to absolute URLs by
    // browsers (in contrast to img.src). This means both SSR and DOM code could
    // contain relative or absolute URLs.
    (src_url_equal(element_urls[i][0], url) || src_url_equal(url, element_urls[i][0]))
  );
}

// node_modules/svelte/src/internal/client/dom/elements/customizable-select.js
var supported = null;
function is_supported() {
  var _a, _b;
  if (supported === null) {
    var select = create_element("select");
    select.innerHTML = create_trusted_html("<option><span>t</span></option>");
    supported = /** @type {Element} */
    ((_b = (_a = select.firstChild) == null ? void 0 : _a.firstChild) == null ? void 0 : _b.nodeType) === 1;
  }
  return supported;
}
function selectedcontent(element2, update_element) {
  if (!is_supported()) return;
  attach(element2, () => () => {
    const select = element2.closest("select");
    if (!select) return;
    const observer = new MutationObserver((entries) => {
      var _a, _b;
      var selected = false;
      for (const entry of entries) {
        if (entry.target === element2) {
          return;
        }
        selected || (selected = !!((_b = (_a = entry.target.parentElement) == null ? void 0 : _a.closest("option")) == null ? void 0 : _b.selected));
      }
      if (selected) {
        element2.replaceWith(element2 = /** @type {HTMLElement} */
        element2.cloneNode(true));
        update_element(element2);
      }
    });
    observer.observe(select, {
      childList: true,
      characterData: true,
      subtree: true
    });
    return () => {
      observer.disconnect();
    };
  });
}
function customizable_select(element2, rich_fn) {
  var was_hydrating = hydrating;
  if (!is_supported()) {
    set_hydrating(false);
    element2.textContent = "";
    element2.append(create_comment(""));
  }
  try {
    rich_fn();
  } finally {
    if (was_hydrating) {
      if (hydrating) {
        reset(element2);
      } else {
        set_hydrating(true);
        set_hydrate_node(element2);
      }
    }
  }
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/document.js
function bind_active_element(update2) {
  listen(document, ["focusin", "focusout"], (event2) => {
    if (event2 && event2.type === "focusout" && /** @type {FocusEvent} */
    event2.relatedTarget) {
      return;
    }
    update2(document.activeElement);
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/input.js
function bind_value(input, get2, set2 = get2) {
  var batches = /* @__PURE__ */ new WeakSet();
  listen_to_event_and_reset_event(input, "input", async (is_reset) => {
    if (true_default && input.type === "checkbox") {
      bind_invalid_checkbox_value();
    }
    var value = is_reset ? input.defaultValue : input.value;
    value = is_numberlike_input(input) ? to_number(value) : value;
    set2(value);
    if (current_batch !== null) {
      batches.add(current_batch);
    }
    await tick();
    if (value !== (value = get2())) {
      var start = input.selectionStart;
      var end = input.selectionEnd;
      var length = input.value.length;
      input.value = value ?? "";
      if (end !== null) {
        var new_length = input.value.length;
        if (start === end && end === length && new_length > length) {
          input.selectionStart = new_length;
          input.selectionEnd = new_length;
        } else {
          input.selectionStart = start;
          input.selectionEnd = Math.min(end, new_length);
        }
      }
    }
  });
  if (
    // If we are hydrating and the value has since changed,
    // then use the updated value from the input instead.
    hydrating && input.defaultValue !== input.value || // If defaultValue is set, then value == defaultValue
    // TODO Svelte 6: remove input.value check and set to empty string?
    untrack(get2) == null && input.value
  ) {
    set2(is_numberlike_input(input) ? to_number(input.value) : input.value);
    if (current_batch !== null) {
      batches.add(current_batch);
    }
  }
  render_effect(() => {
    if (true_default && input.type === "checkbox") {
      bind_invalid_checkbox_value();
    }
    var value = get2();
    if (input === document.activeElement) {
      var batch = (
        /** @type {Batch} */
        async_mode_flag ? previous_batch : current_batch
      );
      if (batches.has(batch)) {
        return;
      }
    }
    if (is_numberlike_input(input) && value === to_number(input.value)) {
      return;
    }
    if (input.type === "date" && !value && !input.value) {
      return;
    }
    if (value !== input.value) {
      input.value = value ?? "";
    }
  });
}
var pending = /* @__PURE__ */ new Set();
function bind_group(inputs, group_index, input, get2, set2 = get2) {
  var is_checkbox = input.getAttribute("type") === "checkbox";
  var binding_group = inputs;
  let hydration_mismatch2 = false;
  if (group_index !== null) {
    for (var index2 of group_index) {
      binding_group = binding_group[index2] ?? (binding_group[index2] = []);
    }
  }
  binding_group.push(input);
  listen_to_event_and_reset_event(
    input,
    "change",
    () => {
      var value = input.__value;
      if (is_checkbox) {
        value = get_binding_group_value(binding_group, value, input.checked);
      }
      set2(value);
    },
    // TODO better default value handling
    () => set2(is_checkbox ? [] : null)
  );
  render_effect(() => {
    var value = get2();
    if (hydrating && input.defaultChecked !== input.checked) {
      hydration_mismatch2 = true;
      return;
    }
    if (is_checkbox) {
      value = value || [];
      input.checked = value.includes(input.__value);
    } else {
      input.checked = is(input.__value, value);
    }
  });
  teardown(() => {
    var index3 = binding_group.indexOf(input);
    if (index3 !== -1) {
      binding_group.splice(index3, 1);
    }
  });
  if (!pending.has(binding_group)) {
    pending.add(binding_group);
    queue_micro_task(() => {
      binding_group.sort((a, b) => a.compareDocumentPosition(b) === 4 ? -1 : 1);
      pending.delete(binding_group);
    });
  }
  queue_micro_task(() => {
    if (hydration_mismatch2) {
      var value;
      if (is_checkbox) {
        value = get_binding_group_value(binding_group, value, input.checked);
      } else {
        var hydration_input = binding_group.find((input2) => input2.checked);
        value = hydration_input == null ? void 0 : hydration_input.__value;
      }
      set2(value);
    }
  });
}
function bind_checked(input, get2, set2 = get2) {
  listen_to_event_and_reset_event(input, "change", (is_reset) => {
    var value = is_reset ? input.defaultChecked : input.checked;
    set2(value);
  });
  if (
    // If we are hydrating and the value has since changed,
    // then use the update value from the input instead.
    hydrating && input.defaultChecked !== input.checked || // If defaultChecked is set, then checked == defaultChecked
    untrack(get2) == null
  ) {
    set2(input.checked);
  }
  render_effect(() => {
    var value = get2();
    input.checked = Boolean(value);
  });
}
function get_binding_group_value(group, __value, checked) {
  var value = /* @__PURE__ */ new Set();
  for (var i = 0; i < group.length; i += 1) {
    if (group[i].checked) {
      value.add(group[i].__value);
    }
  }
  if (!checked) {
    value.delete(__value);
  }
  return Array.from(value);
}
function is_numberlike_input(input) {
  var type = input.type;
  return type === "number" || type === "range";
}
function to_number(value) {
  return value === "" ? null : +value;
}
function bind_files(input, get2, set2 = get2) {
  listen_to_event_and_reset_event(input, "change", () => {
    set2(input.files);
  });
  if (
    // If we are hydrating and the value has since changed,
    // then use the updated value from the input instead.
    hydrating && input.files
  ) {
    set2(input.files);
  }
  render_effect(() => {
    input.files = get2();
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/media.js
function time_ranges_to_array(ranges) {
  var array = [];
  for (var i = 0; i < ranges.length; i += 1) {
    array.push({ start: ranges.start(i), end: ranges.end(i) });
  }
  return array;
}
function bind_current_time(media, get2, set2 = get2) {
  var raf_id;
  var value;
  var callback = () => {
    cancelAnimationFrame(raf_id);
    if (!media.paused) {
      raf_id = requestAnimationFrame(callback);
    }
    var next_value = media.currentTime;
    if (value !== next_value) {
      set2(value = next_value);
    }
  };
  raf_id = requestAnimationFrame(callback);
  media.addEventListener("timeupdate", callback);
  render_effect(() => {
    var next_value = Number(get2());
    if (value !== next_value && !isNaN(
      /** @type {any} */
      next_value
    )) {
      media.currentTime = value = next_value;
    }
  });
  teardown(() => {
    cancelAnimationFrame(raf_id);
    media.removeEventListener("timeupdate", callback);
  });
}
function bind_buffered(media, set2) {
  var current;
  listen(media, ["loadedmetadata", "progress", "timeupdate", "seeking"], () => {
    var ranges = media.buffered;
    if (!current || current.length !== ranges.length || current.some((range, i) => ranges.start(i) !== range.start || ranges.end(i) !== range.end)) {
      current = time_ranges_to_array(ranges);
      set2(current);
    }
  });
}
function bind_seekable(media, set2) {
  listen(media, ["loadedmetadata"], () => set2(time_ranges_to_array(media.seekable)));
}
function bind_played(media, set2) {
  listen(media, ["timeupdate"], () => set2(time_ranges_to_array(media.played)));
}
function bind_seeking(media, set2) {
  listen(media, ["seeking", "seeked"], () => set2(media.seeking));
}
function bind_ended(media, set2) {
  listen(media, ["timeupdate", "ended"], () => set2(media.ended));
}
function bind_ready_state(media, set2) {
  listen(
    media,
    ["loadedmetadata", "loadeddata", "canplay", "canplaythrough", "playing", "waiting", "emptied"],
    () => set2(media.readyState)
  );
}
function bind_playback_rate(media, get2, set2 = get2) {
  effect(() => {
    var value = Number(get2());
    if (value !== media.playbackRate && !isNaN(value)) {
      media.playbackRate = value;
    }
  });
  effect(() => {
    listen(media, ["ratechange"], () => {
      set2(media.playbackRate);
    });
  });
}
function bind_paused(media, get2, set2 = get2) {
  var paused = get2();
  var update2 = () => {
    if (paused !== media.paused) {
      set2(paused = media.paused);
    }
  };
  listen(media, ["play", "pause", "canplay"], update2, paused == null);
  effect(() => {
    if ((paused = !!get2()) !== media.paused) {
      if (paused) {
        media.pause();
      } else {
        media.play().catch((error) => {
          set2(paused = true);
          throw error;
        });
      }
    }
  });
}
function bind_volume(media, get2, set2 = get2) {
  var callback = () => {
    set2(media.volume);
  };
  if (get2() == null) {
    callback();
  }
  listen(media, ["volumechange"], callback, false);
  render_effect(() => {
    var value = Number(get2());
    if (value !== media.volume && !isNaN(value)) {
      media.volume = value;
    }
  });
}
function bind_muted(media, get2, set2 = get2) {
  var callback = () => {
    set2(media.muted);
  };
  if (get2() == null) {
    callback();
  }
  listen(media, ["volumechange"], callback, false);
  render_effect(() => {
    var value = !!get2();
    if (media.muted !== value) media.muted = value;
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/navigator.js
function bind_online(update2) {
  listen(window, ["online", "offline"], () => {
    update2(navigator.onLine);
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/props.js
function bind_prop(props, prop2, value) {
  var desc = get_descriptor(props, prop2);
  if (desc && desc.set) {
    props[prop2] = value;
    teardown(() => {
      props[prop2] = null;
    });
  }
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/size.js
var _listeners, _observer, _options, _ResizeObserverSingleton_instances, getObserver_fn;
var _ResizeObserverSingleton = class _ResizeObserverSingleton {
  /** @param {ResizeObserverOptions} options */
  constructor(options) {
    __privateAdd(this, _ResizeObserverSingleton_instances);
    /** */
    __privateAdd(this, _listeners, /* @__PURE__ */ new WeakMap());
    /** @type {ResizeObserver | undefined} */
    __privateAdd(this, _observer);
    /** @type {ResizeObserverOptions} */
    __privateAdd(this, _options);
    __privateSet(this, _options, options);
  }
  /**
   * @param {Element} element
   * @param {(entry: ResizeObserverEntry) => any} listener
   */
  observe(element2, listener) {
    var listeners = __privateGet(this, _listeners).get(element2) || /* @__PURE__ */ new Set();
    listeners.add(listener);
    __privateGet(this, _listeners).set(element2, listeners);
    __privateMethod(this, _ResizeObserverSingleton_instances, getObserver_fn).call(this).observe(element2, __privateGet(this, _options));
    return () => {
      var listeners2 = __privateGet(this, _listeners).get(element2);
      listeners2.delete(listener);
      if (listeners2.size === 0) {
        __privateGet(this, _listeners).delete(element2);
        __privateGet(this, _observer).unobserve(element2);
      }
    };
  }
};
_listeners = new WeakMap();
_observer = new WeakMap();
_options = new WeakMap();
_ResizeObserverSingleton_instances = new WeakSet();
getObserver_fn = function() {
  return __privateGet(this, _observer) ?? __privateSet(this, _observer, new ResizeObserver(
    /** @param {any} entries */
    (entries) => {
      for (var entry of entries) {
        _ResizeObserverSingleton.entries.set(entry.target, entry);
        for (var listener of __privateGet(this, _listeners).get(entry.target) || []) {
          listener(entry);
        }
      }
    }
  ));
};
/** @static */
__publicField(_ResizeObserverSingleton, "entries", /* @__PURE__ */ new WeakMap());
var ResizeObserverSingleton = _ResizeObserverSingleton;
var resize_observer_content_box = new ResizeObserverSingleton({
  box: "content-box"
});
var resize_observer_border_box = new ResizeObserverSingleton({
  box: "border-box"
});
var resize_observer_device_pixel_content_box = new ResizeObserverSingleton({
  box: "device-pixel-content-box"
});
function bind_resize_observer(element2, type, set2) {
  var observer = type === "contentRect" || type === "contentBoxSize" ? resize_observer_content_box : type === "borderBoxSize" ? resize_observer_border_box : resize_observer_device_pixel_content_box;
  var unsub = observer.observe(
    element2,
    /** @param {any} entry */
    (entry) => set2(entry[type])
  );
  teardown(unsub);
}
function bind_element_size(element2, type, set2) {
  var unsub = resize_observer_border_box.observe(element2, () => set2(element2[type]));
  effect(() => {
    untrack(() => set2(element2[type]));
    return unsub;
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/this.js
function is_bound_this(bound_value, element_or_component) {
  return bound_value === element_or_component || (bound_value == null ? void 0 : bound_value[STATE_SYMBOL]) === element_or_component;
}
function bind_this(element_or_component = {}, update2, get_value, get_parts) {
  var component_effect = (
    /** @type {ComponentContext} */
    component_context.r
  );
  var parent = (
    /** @type {Effect} */
    active_effect
  );
  effect(() => {
    var old_parts;
    var parts;
    render_effect(() => {
      old_parts = parts;
      parts = (get_parts == null ? void 0 : get_parts()) || [];
      untrack(() => {
        if (!is_bound_this(get_value(...parts), element_or_component)) {
          update2(element_or_component, ...parts);
          if (old_parts && is_bound_this(get_value(...old_parts), element_or_component)) {
            update2(null, ...old_parts);
          }
        }
      });
    });
    return () => {
      let p = parent;
      while (p !== component_effect && p.parent !== null && p.parent.f & DESTROYING) {
        p = p.parent;
      }
      const teardown2 = () => {
        if (parts && is_bound_this(get_value(...parts), element_or_component)) {
          update2(null, ...parts);
        }
      };
      const original_teardown = p.teardown;
      p.teardown = () => {
        teardown2();
        original_teardown == null ? void 0 : original_teardown();
      };
    };
  });
  return element_or_component;
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/universal.js
function bind_content_editable(property, element2, get2, set2 = get2) {
  element2.addEventListener("input", () => {
    set2(element2[property]);
  });
  render_effect(() => {
    var value = get2();
    if (element2[property] !== value) {
      if (value == null) {
        var non_null_value = element2[property];
        set2(non_null_value);
      } else {
        element2[property] = value + "";
      }
    }
  });
}
function bind_property(property, event_name, element2, set2, get2) {
  var handler = () => {
    set2(element2[property]);
  };
  element2.addEventListener(event_name, handler);
  if (get2) {
    render_effect(() => {
      element2[property] = get2();
    });
  } else {
    handler();
  }
  if (element2 === document.body || element2 === window || element2 === document) {
    teardown(() => {
      element2.removeEventListener(event_name, handler);
    });
  }
}
function bind_focused(element2, set2) {
  listen(element2, ["focus", "blur"], () => {
    set2(element2 === document.activeElement);
  });
}

// node_modules/svelte/src/internal/client/dom/elements/bindings/window.js
function bind_window_scroll(type, get2, set2 = get2) {
  var is_scrolling_x = type === "x";
  var target_handler = () => without_reactive_context(() => {
    scrolling = true;
    clearTimeout(timeout);
    timeout = setTimeout(clear, 100);
    set2(window[is_scrolling_x ? "scrollX" : "scrollY"]);
  });
  addEventListener("scroll", target_handler, {
    passive: true
  });
  var scrolling = false;
  var timeout;
  var clear = () => {
    scrolling = false;
  };
  var first = true;
  render_effect(() => {
    var latest_value = get2();
    if (first) {
      first = false;
    } else if (!scrolling && latest_value != null) {
      scrolling = true;
      clearTimeout(timeout);
      if (is_scrolling_x) {
        scrollTo(latest_value, window.scrollY);
      } else {
        scrollTo(window.scrollX, latest_value);
      }
      timeout = setTimeout(clear, 100);
    }
  });
  effect(target_handler);
  teardown(() => {
    removeEventListener("scroll", target_handler);
  });
}
function bind_window_size(type, set2) {
  listen(window, ["resize"], () => without_reactive_context(() => set2(window[type])));
}

// node_modules/svelte/src/internal/client/dom/legacy/lifecycle.js
function init(immutable = false) {
  const context = (
    /** @type {ComponentContextLegacy} */
    component_context
  );
  const callbacks = context.l.u;
  if (!callbacks) return;
  let props = () => deep_read_state(context.s);
  if (immutable) {
    let version = 0;
    let prev = (
      /** @type {Record<string, any>} */
      {}
    );
    const d = derived(() => {
      let changed = false;
      const props2 = context.s;
      for (const key2 in props2) {
        if (props2[key2] !== prev[key2]) {
          prev[key2] = props2[key2];
          changed = true;
        }
      }
      if (changed) version++;
      return version;
    });
    props = () => get(d);
  }
  if (callbacks.b.length) {
    user_pre_effect(() => {
      observe_all(context, props);
      run_all(callbacks.b);
    });
  }
  user_effect(() => {
    const fns = untrack(() => callbacks.m.map(run));
    return () => {
      for (const fn of fns) {
        if (typeof fn === "function") {
          fn();
        }
      }
    };
  });
  if (callbacks.a.length) {
    user_effect(() => {
      observe_all(context, props);
      run_all(callbacks.a);
    });
  }
}
function observe_all(context, props) {
  if (context.l.s) {
    for (const signal of context.l.s) get(signal);
  }
  props();
}

// node_modules/svelte/src/internal/client/dom/legacy/misc.js
function reactive_import(fn) {
  var s = source(0);
  return function() {
    if (arguments.length === 1) {
      set(s, get(s) + 1);
      return arguments[0];
    } else {
      get(s);
      return fn();
    }
  };
}
function bubble_event($$props, event2) {
  var _a;
  var events = (
    /** @type {Record<string, Function[] | Function>} */
    (_a = $$props.$$events) == null ? void 0 : _a[event2.type]
  );
  var callbacks = is_array(events) ? events.slice() : events == null ? [] : [events];
  for (var fn of callbacks) {
    fn.call(this, event2);
  }
}
function add_legacy_event_listener($$props, event_name, event_callback) {
  var _a;
  $$props.$$events || ($$props.$$events = {});
  (_a = $$props.$$events)[event_name] || (_a[event_name] = []);
  $$props.$$events[event_name].push(event_callback);
}
function update_legacy_props($$new_props) {
  for (var key2 in $$new_props) {
    if (key2 in this) {
      this[key2] = $$new_props[key2];
    }
  }
}

// node_modules/svelte/src/internal/client/reactivity/props.js
function update_prop(fn, d = 1) {
  const value = fn();
  fn(value + d);
  return value;
}
function update_pre_prop(fn, d = 1) {
  const value = fn() + d;
  fn(value);
  return value;
}
var rest_props_handler = {
  get(target, key2) {
    if (target.exclude.includes(key2)) return;
    return target.props[key2];
  },
  set(target, key2) {
    if (true_default) {
      props_rest_readonly(`${target.name}.${String(key2)}`);
    }
    return false;
  },
  getOwnPropertyDescriptor(target, key2) {
    if (target.exclude.includes(key2)) return;
    if (key2 in target.props) {
      return {
        enumerable: true,
        configurable: true,
        value: target.props[key2]
      };
    }
  },
  has(target, key2) {
    if (target.exclude.includes(key2)) return false;
    return key2 in target.props;
  },
  ownKeys(target) {
    return Reflect.ownKeys(target.props).filter((key2) => !target.exclude.includes(key2));
  }
};
function rest_props(props, exclude, name) {
  return new Proxy(
    true_default ? { props, exclude, name, other: {}, to_proxy: [] } : { props, exclude },
    rest_props_handler
  );
}
var legacy_rest_props_handler = {
  get(target, key2) {
    if (target.exclude.includes(key2)) return;
    get(target.version);
    return key2 in target.special ? target.special[key2]() : target.props[key2];
  },
  set(target, key2, value) {
    if (!(key2 in target.special)) {
      var previous_effect = active_effect;
      try {
        set_active_effect(target.parent_effect);
        target.special[key2] = prop(
          {
            get [key2]() {
              return target.props[key2];
            }
          },
          /** @type {string} */
          key2,
          PROPS_IS_UPDATED
        );
      } finally {
        set_active_effect(previous_effect);
      }
    }
    target.special[key2](value);
    update(target.version);
    return true;
  },
  getOwnPropertyDescriptor(target, key2) {
    if (target.exclude.includes(key2)) return;
    if (key2 in target.props) {
      return {
        enumerable: true,
        configurable: true,
        value: target.props[key2]
      };
    }
  },
  deleteProperty(target, key2) {
    if (target.exclude.includes(key2)) return true;
    target.exclude.push(key2);
    update(target.version);
    return true;
  },
  has(target, key2) {
    if (target.exclude.includes(key2)) return false;
    return key2 in target.props;
  },
  ownKeys(target) {
    return Reflect.ownKeys(target.props).filter((key2) => !target.exclude.includes(key2));
  }
};
function legacy_rest_props(props, exclude) {
  return new Proxy(
    {
      props,
      exclude,
      special: {},
      version: source(0),
      // TODO this is only necessary because we need to track component
      // destruction inside `prop`, because of `bind:this`, but it
      // seems likely that we can simplify `bind:this` instead
      parent_effect: (
        /** @type {Effect} */
        active_effect
      )
    },
    legacy_rest_props_handler
  );
}
var spread_props_handler = {
  get(target, key2) {
    let i = target.props.length;
    while (i--) {
      let p = target.props[i];
      if (is_function(p)) p = p();
      if (typeof p === "object" && p !== null && key2 in p) return p[key2];
    }
  },
  set(target, key2, value) {
    let i = target.props.length;
    while (i--) {
      let p = target.props[i];
      if (is_function(p)) p = p();
      const desc = get_descriptor(p, key2);
      if (desc && desc.set) {
        desc.set(value);
        return true;
      }
    }
    return false;
  },
  getOwnPropertyDescriptor(target, key2) {
    let i = target.props.length;
    while (i--) {
      let p = target.props[i];
      if (is_function(p)) p = p();
      if (typeof p === "object" && p !== null && key2 in p) {
        const descriptor = get_descriptor(p, key2);
        if (descriptor && !descriptor.configurable) {
          descriptor.configurable = true;
        }
        return descriptor;
      }
    }
  },
  has(target, key2) {
    if (key2 === STATE_SYMBOL || key2 === LEGACY_PROPS) return false;
    for (let p of target.props) {
      if (is_function(p)) p = p();
      if (p != null && key2 in p) return true;
    }
    return false;
  },
  ownKeys(target) {
    const keys = [];
    for (let p of target.props) {
      if (is_function(p)) p = p();
      if (!p) continue;
      for (const key2 in p) {
        if (!keys.includes(key2)) keys.push(key2);
      }
      for (const key2 of Object.getOwnPropertySymbols(p)) {
        if (!keys.includes(key2)) keys.push(key2);
      }
    }
    return keys;
  }
};
function spread_props(...props) {
  return new Proxy({ props }, spread_props_handler);
}
function prop(props, key2, flags, fallback2) {
  var _a;
  var runes = !legacy_mode_flag || (flags & PROPS_IS_RUNES) !== 0;
  var bindable = (flags & PROPS_IS_BINDABLE) !== 0;
  var lazy = (flags & PROPS_IS_LAZY_INITIAL) !== 0;
  var fallback_value = (
    /** @type {V} */
    fallback2
  );
  var fallback_dirty = true;
  var fallback_signal = (
    /** @type {Derived<V> | undefined} */
    void 0
  );
  var get_fallback = () => {
    if (lazy && runes) {
      fallback_signal ?? (fallback_signal = derived(
        /** @type {() => V} */
        fallback2
      ));
      return get(fallback_signal);
    }
    if (fallback_dirty) {
      fallback_dirty = false;
      fallback_value = lazy ? untrack(
        /** @type {() => V} */
        fallback2
      ) : (
        /** @type {V} */
        fallback2
      );
    }
    return fallback_value;
  };
  let setter;
  if (bindable) {
    var is_entry_props = STATE_SYMBOL in props || LEGACY_PROPS in props;
    setter = ((_a = get_descriptor(props, key2)) == null ? void 0 : _a.set) ?? (is_entry_props && key2 in props ? (v) => props[key2] = v : void 0);
  }
  var initial_value;
  var is_store_sub = false;
  if (bindable) {
    [initial_value, is_store_sub] = capture_store_binding(() => (
      /** @type {V} */
      props[key2]
    ));
  } else {
    initial_value = /** @type {V} */
    props[key2];
  }
  if (initial_value === void 0 && fallback2 !== void 0) {
    initial_value = get_fallback();
    if (setter) {
      if (runes) props_invalid_value(key2);
      setter(initial_value);
    }
  }
  var getter;
  if (runes) {
    getter = () => {
      var value = (
        /** @type {V} */
        props[key2]
      );
      if (value === void 0) return get_fallback();
      fallback_dirty = true;
      return value;
    };
  } else {
    getter = () => {
      var value = (
        /** @type {V} */
        props[key2]
      );
      if (value !== void 0) {
        fallback_value = /** @type {V} */
        void 0;
      }
      return value === void 0 ? fallback_value : value;
    };
  }
  if (runes && (flags & PROPS_IS_UPDATED) === 0) {
    return getter;
  }
  if (setter) {
    var legacy_parent = props.$$legacy;
    return (
      /** @type {() => V} */
      (function(value, mutation) {
        if (arguments.length > 0) {
          if (!runes || !mutation || legacy_parent || is_store_sub) {
            setter(mutation ? getter() : value);
          }
          return value;
        }
        return getter();
      })
    );
  }
  var overridden = false;
  var d = ((flags & PROPS_IS_IMMUTABLE) !== 0 ? derived : derived_safe_equal)(() => {
    overridden = false;
    return getter();
  });
  if (true_default) {
    d.label = key2;
  }
  if (bindable) get(d);
  var parent_effect = (
    /** @type {Effect} */
    active_effect
  );
  return (
    /** @type {() => V} */
    (function(value, mutation) {
      if (arguments.length > 0) {
        const new_value = mutation ? get(d) : runes && bindable ? proxy(value) : value;
        set(d, new_value);
        overridden = true;
        if (fallback_value !== void 0) {
          fallback_value = new_value;
        }
        return value;
      }
      if (is_destroying_effect && overridden || (parent_effect.f & DESTROYED) !== 0) {
        return d.v;
      }
      return get(d);
    })
  );
}

// node_modules/svelte/src/internal/client/validate.js
function validate_binding(binding, blockers, get_object, get_property, line, column) {
  run_after_blockers(blockers, () => {
    var _a;
    var warned = false;
    var filename = (_a = dev_current_component_function) == null ? void 0 : _a[FILENAME];
    render_effect(() => {
      if (warned) return;
      var [object, is_store_sub] = capture_store_binding(get_object);
      if (is_store_sub) return;
      var property = get_property();
      var ran = false;
      var effect2 = render_effect(() => {
        if (ran) return;
        object[property];
      });
      ran = true;
      if (effect2.deps === null) {
        var location = `${filename}:${line}:${column}`;
        binding_property_non_reactive(binding, location);
        warned = true;
      }
    });
  });
}

// node_modules/svelte/src/internal/client/dom/elements/custom-element.js
var SvelteElement;
if (typeof HTMLElement === "function") {
  SvelteElement = class extends HTMLElement {
    /**
     * @param {*} $$componentCtor
     * @param {*} $$slots
     * @param {ShadowRootInit | undefined} shadow_root_init
     */
    constructor($$componentCtor, $$slots, shadow_root_init) {
      super();
      /** The Svelte component constructor */
      __publicField(this, "$$ctor");
      /** Slots */
      __publicField(this, "$$s");
      /** @type {any} The Svelte component instance */
      __publicField(this, "$$c");
      /** Whether or not the custom element is connected */
      __publicField(this, "$$cn", false);
      /** @type {Record<string, any>} Component props data */
      __publicField(this, "$$d", {});
      /** `true` if currently in the process of reflecting component props back to attributes */
      __publicField(this, "$$r", false);
      /** @type {Record<string, CustomElementPropDefinition>} Props definition (name, reflected, type etc) */
      __publicField(this, "$$p_d", {});
      /** @type {Record<string, EventListenerOrEventListenerObject[]>} Event listeners */
      __publicField(this, "$$l", {});
      /** @type {Map<EventListenerOrEventListenerObject, Function>} Event listener unsubscribe functions */
      __publicField(this, "$$l_u", /* @__PURE__ */ new Map());
      /** @type {any} The managed render effect for reflecting attributes */
      __publicField(this, "$$me");
      /** @type {ShadowRoot | null} The ShadowRoot of the custom element */
      __publicField(this, "$$shadowRoot", null);
      this.$$ctor = $$componentCtor;
      this.$$s = $$slots;
      if (shadow_root_init) {
        this.$$shadowRoot = this.attachShadow(shadow_root_init);
      }
    }
    /**
     * @param {string} type
     * @param {EventListenerOrEventListenerObject} listener
     * @param {boolean | AddEventListenerOptions} [options]
     */
    addEventListener(type, listener, options) {
      this.$$l[type] = this.$$l[type] || [];
      this.$$l[type].push(listener);
      if (this.$$c) {
        const unsub = this.$$c.$on(type, listener);
        this.$$l_u.set(listener, unsub);
      }
      super.addEventListener(type, listener, options);
    }
    /**
     * @param {string} type
     * @param {EventListenerOrEventListenerObject} listener
     * @param {boolean | AddEventListenerOptions} [options]
     */
    removeEventListener(type, listener, options) {
      super.removeEventListener(type, listener, options);
      if (this.$$c) {
        const unsub = this.$$l_u.get(listener);
        if (unsub) {
          unsub();
          this.$$l_u.delete(listener);
        }
      }
    }
    async connectedCallback() {
      this.$$cn = true;
      if (!this.$$c) {
        let create_slot = function(name) {
          return (anchor) => {
            const slot2 = create_element("slot");
            if (name !== "default") slot2.name = name;
            append(anchor, slot2);
          };
        };
        await Promise.resolve();
        if (!this.$$cn || this.$$c) {
          return;
        }
        const $$slots = {};
        const existing_slots = get_custom_elements_slots(this);
        for (const name of this.$$s) {
          if (name in existing_slots) {
            if (name === "default" && !this.$$d.children) {
              this.$$d.children = create_slot(name);
              $$slots.default = true;
            } else {
              $$slots[name] = create_slot(name);
            }
          }
        }
        for (const attribute of this.attributes) {
          const name = this.$$g_p(attribute.name);
          if (!(name in this.$$d)) {
            this.$$d[name] = get_custom_element_value(name, attribute.value, this.$$p_d, "toProp");
          }
        }
        for (const key2 in this.$$p_d) {
          if (!(key2 in this.$$d) && this[key2] !== void 0) {
            this.$$d[key2] = this[key2];
            delete this[key2];
          }
        }
        this.$$c = createClassComponent({
          component: this.$$ctor,
          target: this.$$shadowRoot || this,
          props: {
            ...this.$$d,
            $$slots,
            $$host: this
          }
        });
        this.$$me = effect_root(() => {
          render_effect(() => {
            var _a;
            this.$$r = true;
            for (const key2 of object_keys(this.$$c)) {
              if (!((_a = this.$$p_d[key2]) == null ? void 0 : _a.reflect)) continue;
              this.$$d[key2] = this.$$c[key2];
              const attribute_value = get_custom_element_value(
                key2,
                this.$$d[key2],
                this.$$p_d,
                "toAttribute"
              );
              if (attribute_value == null) {
                this.removeAttribute(this.$$p_d[key2].attribute || key2);
              } else {
                this.setAttribute(this.$$p_d[key2].attribute || key2, attribute_value);
              }
            }
            this.$$r = false;
          });
        });
        for (const type in this.$$l) {
          for (const listener of this.$$l[type]) {
            const unsub = this.$$c.$on(type, listener);
            this.$$l_u.set(listener, unsub);
          }
        }
        this.$$l = {};
      }
    }
    // We don't need this when working within Svelte code, but for compatibility of people using this outside of Svelte
    // and setting attributes through setAttribute etc, this is helpful
    /**
     * @param {string} attr
     * @param {string} _oldValue
     * @param {string} newValue
     */
    attributeChangedCallback(attr2, _oldValue, newValue) {
      var _a;
      if (this.$$r) return;
      attr2 = this.$$g_p(attr2);
      this.$$d[attr2] = get_custom_element_value(attr2, newValue, this.$$p_d, "toProp");
      (_a = this.$$c) == null ? void 0 : _a.$set({ [attr2]: this.$$d[attr2] });
    }
    disconnectedCallback() {
      this.$$cn = false;
      Promise.resolve().then(() => {
        if (!this.$$cn && this.$$c) {
          this.$$c.$destroy();
          this.$$me();
          this.$$c = void 0;
        }
      });
    }
    /**
     * @param {string} attribute_name
     */
    $$g_p(attribute_name) {
      return object_keys(this.$$p_d).find(
        (key2) => this.$$p_d[key2].attribute === attribute_name || !this.$$p_d[key2].attribute && key2.toLowerCase() === attribute_name
      ) || attribute_name;
    }
  };
}
function get_custom_element_value(prop2, value, props_definition, transform) {
  var _a;
  const type = (_a = props_definition[prop2]) == null ? void 0 : _a.type;
  value = type === "Boolean" && typeof value !== "boolean" ? value != null : value;
  if (!transform || !props_definition[prop2]) {
    return value;
  } else if (transform === "toAttribute") {
    switch (type) {
      case "Object":
      case "Array":
        return value == null ? null : JSON.stringify(value);
      case "Boolean":
        return value ? "" : null;
      case "Number":
        return value == null ? null : value;
      default:
        return value;
    }
  } else {
    switch (type) {
      case "Object":
      case "Array":
        return value && JSON.parse(value);
      case "Boolean":
        return value;
      // conversion already handled above
      case "Number":
        return value != null ? +value : value;
      default:
        return value;
    }
  }
}
function get_custom_elements_slots(element2) {
  const result = {};
  element2.childNodes.forEach((node) => {
    result[
      /** @type {Element} node */
      node.slot || "default"
    ] = true;
  });
  return result;
}
function create_custom_element(Component, props_definition, slots, exports, shadow_root_init, extend) {
  let Class = class extends SvelteElement {
    constructor() {
      super(Component, slots, shadow_root_init);
      this.$$p_d = props_definition;
    }
    static get observedAttributes() {
      return object_keys(props_definition).map(
        (key2) => (props_definition[key2].attribute || key2).toLowerCase()
      );
    }
  };
  object_keys(props_definition).forEach((prop2) => {
    define_property(Class.prototype, prop2, {
      get() {
        return this.$$c && prop2 in this.$$c ? this.$$c[prop2] : this.$$d[prop2];
      },
      set(value) {
        var _a;
        value = get_custom_element_value(prop2, value, props_definition);
        this.$$d[prop2] = value;
        var component2 = this.$$c;
        if (component2) {
          var setter = (_a = get_descriptor(component2, prop2)) == null ? void 0 : _a.get;
          if (setter) {
            component2[prop2] = value;
          } else {
            component2.$set({ [prop2]: value });
          }
        }
      }
    });
  });
  exports.forEach((property) => {
    define_property(Class.prototype, property, {
      get() {
        var _a;
        return (_a = this.$$c) == null ? void 0 : _a[property];
      }
    });
  });
  if (extend) {
    Class = extend(Class);
  }
  Component.element = /** @type {any} */
  Class;
  return Class;
}

// node_modules/svelte/src/internal/client/dev/console-log.js
function log_if_contains_state(method, ...objects) {
  untrack(() => {
    try {
      let has_state = false;
      const transformed = [];
      for (const obj of objects) {
        if (obj && typeof obj === "object" && STATE_SYMBOL in obj) {
          transformed.push(snapshot(obj, true));
          has_state = true;
        } else {
          transformed.push(obj);
        }
      }
      if (has_state) {
        console_log_state(method);
        console.log("%c[snapshot]", "color: grey", ...transformed);
      }
    } catch {
    }
  });
  return objects;
}

// node_modules/svelte/src/internal/client/hydratable.js
function hydratable(key2, fn) {
  var _a;
  if (!async_mode_flag) {
    experimental_async_required("hydratable");
  }
  if (hydrating) {
    const store = (_a = window.__svelte) == null ? void 0 : _a.h;
    if (store == null ? void 0 : store.has(key2)) {
      return (
        /** @type {T} */
        store.get(key2)
      );
    }
    if (true_default) {
      hydratable_missing_but_required(key2);
    } else {
      hydratable_missing_but_expected(key2);
    }
  }
  return fn();
}

// node_modules/svelte/src/index-client.js
if (true_default) {
  let throw_rune_error = function(rune) {
    if (!(rune in globalThis)) {
      let value;
      Object.defineProperty(globalThis, rune, {
        configurable: true,
        // eslint-disable-next-line getter-return
        get: () => {
          if (value !== void 0) {
            return value;
          }
          rune_outside_svelte(rune);
        },
        set: (v) => {
          value = v;
        }
      });
    }
  };
  throw_rune_error("$state");
  throw_rune_error("$effect");
  throw_rune_error("$derived");
  throw_rune_error("$inspect");
  throw_rune_error("$props");
  throw_rune_error("$bindable");
}
function getAbortSignal() {
  var _a;
  if (active_reaction === null) {
    get_abort_signal_outside_reaction();
  }
  return ((_a = active_reaction).ac ?? (_a.ac = new AbortController())).signal;
}
function onMount(fn) {
  if (component_context === null) {
    lifecycle_outside_component("onMount");
  }
  if (legacy_mode_flag && component_context.l !== null) {
    init_update_callbacks(component_context).m.push(fn);
  } else {
    user_effect(() => {
      const cleanup = untrack(fn);
      if (typeof cleanup === "function") return (
        /** @type {() => void} */
        cleanup
      );
    });
  }
}
function onDestroy(fn) {
  if (component_context === null) {
    lifecycle_outside_component("onDestroy");
  }
  onMount(() => () => untrack(fn));
}
function create_custom_event(type, detail, { bubbles = false, cancelable = false } = {}) {
  return new CustomEvent(type, { detail, bubbles, cancelable });
}
function createEventDispatcher() {
  const active_component_context = component_context;
  if (active_component_context === null) {
    lifecycle_outside_component("createEventDispatcher");
  }
  return (type, detail, options) => {
    var _a;
    const events = (
      /** @type {Record<string, Function | Function[]>} */
      (_a = active_component_context.s.$$events) == null ? void 0 : _a[
        /** @type {string} */
        type
      ]
    );
    if (events) {
      const callbacks = is_array(events) ? events.slice() : [events];
      const event2 = create_custom_event(
        /** @type {string} */
        type,
        detail,
        options
      );
      for (const fn of callbacks) {
        fn.call(active_component_context.x, event2);
      }
      return !event2.defaultPrevented;
    }
    return true;
  };
}
function beforeUpdate(fn) {
  if (component_context === null) {
    lifecycle_outside_component("beforeUpdate");
  }
  if (component_context.l === null) {
    lifecycle_legacy_only("beforeUpdate");
  }
  init_update_callbacks(component_context).b.push(fn);
}
function afterUpdate(fn) {
  if (component_context === null) {
    lifecycle_outside_component("afterUpdate");
  }
  if (component_context.l === null) {
    lifecycle_legacy_only("afterUpdate");
  }
  init_update_callbacks(component_context).a.push(fn);
}
function init_update_callbacks(context) {
  var l = (
    /** @type {ComponentContextLegacy} */
    context.l
  );
  return l.u ?? (l.u = { a: [], b: [], m: [] });
}

// node_modules/svelte/src/attachments/index.js
function createAttachmentKey() {
  return Symbol(ATTACHMENT_KEY);
}
function fromAction(action2, fn = (
  /** @type {() => T} */
  noop
)) {
  return (element2) => {
    const { update: update2, destroy } = untrack(() => action2(element2, fn()) ?? {});
    if (update2) {
      var ran = false;
      render_effect(() => {
        const arg = fn();
        if (ran) update2(arg);
      });
      ran = true;
    }
    if (destroy) {
      teardown(destroy);
    }
  };
}

export {
  createAttachmentKey,
  fromAction,
  assign,
  assign_async,
  cleanup_styles,
  add_locations,
  hmr,
  create_ownership_validator,
  check_target,
  legacy_api,
  inspect,
  async,
  validate_snippet_args,
  await_block,
  if_block,
  key,
  css_props,
  index,
  each,
  html,
  slot,
  sanitize_slots,
  validate_void_dynamic_element,
  validate_dynamic_element_tag,
  validate_store,
  prevent_snippet_stringification,
  snippet,
  wrap_snippet,
  createRawSnippet,
  component,
  raf,
  loop,
  animation,
  transition,
  element,
  head,
  append_styles,
  action,
  attach,
  attr,
  clsx2 as clsx,
  set_class,
  set_style,
  select_option,
  init_select,
  bind_select_value,
  CLASS,
  STYLE,
  remove_input_defaults,
  set_value,
  set_checked,
  set_selected,
  set_default_checked,
  set_default_value,
  set_attribute,
  set_xlink_attribute,
  set_custom_element_data,
  attribute_effect,
  selectedcontent,
  customizable_select,
  bind_active_element,
  bind_value,
  bind_group,
  bind_checked,
  bind_files,
  bind_current_time,
  bind_buffered,
  bind_seekable,
  bind_played,
  bind_seeking,
  bind_ended,
  bind_ready_state,
  bind_playback_rate,
  bind_paused,
  bind_volume,
  bind_muted,
  bind_online,
  bind_prop,
  bind_resize_observer,
  bind_element_size,
  bind_this,
  bind_content_editable,
  bind_property,
  bind_focused,
  bind_window_scroll,
  bind_window_size,
  init,
  reactive_import,
  bubble_event,
  add_legacy_event_listener,
  update_legacy_props,
  update_prop,
  update_pre_prop,
  rest_props,
  legacy_rest_props,
  spread_props,
  prop,
  validate_binding,
  create_custom_element,
  log_if_contains_state,
  hydratable,
  getAbortSignal,
  onMount,
  onDestroy,
  createEventDispatcher,
  beforeUpdate,
  afterUpdate
};
//# sourceMappingURL=chunk-7MHAMVTJ.js.map
