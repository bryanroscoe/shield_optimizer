import {
  all_registered_events,
  handle_event_propagation,
  on,
  root_event_handles
} from "./chunk-YY3KXDTM.js";
import {
  COMMENT_NODE,
  DIRTY,
  DOCUMENT_FRAGMENT_NODE,
  FILENAME,
  HYDRATION_END,
  HYDRATION_ERROR,
  HYDRATION_START,
  IS_XHTML,
  LEGACY_PROPS,
  MAYBE_DIRTY,
  NAMESPACE_MATHML,
  NAMESPACE_SVG,
  REACTION_RAN,
  STATE_SYMBOL,
  TEMPLATE_FRAGMENT,
  TEMPLATE_USE_IMPORT_NODE,
  TEMPLATE_USE_MATHML,
  TEMPLATE_USE_SVG,
  TEXT_CACHE,
  TEXT_NODE,
  active_effect,
  array_from,
  boundary,
  clear_text_content,
  component_context,
  component_root,
  create_comment,
  create_element,
  create_fragment,
  create_text,
  define_property,
  dev_current_component_function,
  flushSync,
  get2 as get,
  get_first_child,
  get_next_sibling,
  hydrate_next,
  hydrate_node,
  hydrating,
  hydration_failed,
  init_operations,
  is_array,
  is_firefox,
  lifecycle_outside_component,
  merge_text_nodes,
  mutable_source,
  noop,
  pop,
  push,
  set,
  set_attribute,
  set_hydrate_node,
  set_hydrating,
  set_signal_status,
  user_pre_effect
} from "./chunk-RA5JWBT7.js";
import {
  hydration_mismatch,
  legacy_recursive_reactive_block,
  lifecycle_double_unmount,
  state_proxy_unmount
} from "./chunk-RDQBLHQD.js";
import {
  true_default
} from "./chunk-2GRV7DLA.js";
import {
  async_mode_flag
} from "./chunk-6ZD2PQMF.js";
import {
  __privateAdd,
  __privateGet,
  __privateSet
} from "./chunk-UGBVNEQM.js";

// node_modules/svelte/src/internal/client/dom/reconciler.js
var _a;
var policy = (
  // We gotta write it like this because after downleveling the pure comment may end up in the wrong location
  ((_a = globalThis == null ? void 0 : globalThis.window) == null ? void 0 : _a.trustedTypes) && globalThis.window.trustedTypes.createPolicy("svelte-trusted-html", {
    /** @param {string} html */
    createHTML: (html) => {
      return html;
    }
  })
);
function create_trusted_html(html) {
  return (
    /** @type {string} */
    (policy == null ? void 0 : policy.createHTML(html)) ?? html
  );
}
function create_fragment_from_html(html) {
  var elem = create_element("template");
  elem.innerHTML = create_trusted_html(html.replaceAll("<!>", "<!---->"));
  return elem.content;
}

// node_modules/svelte/src/internal/client/dom/template.js
var TEMPLATE_TAG = IS_XHTML ? "template" : "TEMPLATE";
var SCRIPT_TAG = IS_XHTML ? "script" : "SCRIPT";
function assign_nodes(start, end) {
  var effect = (
    /** @type {Effect} */
    active_effect
  );
  if (effect.nodes === null) {
    effect.nodes = { start, end, a: null, t: null };
  }
}
function from_html(content, flags) {
  var is_fragment = (flags & TEMPLATE_FRAGMENT) !== 0;
  var use_import_node = (flags & TEMPLATE_USE_IMPORT_NODE) !== 0;
  var node;
  var has_start = !content.startsWith("<!>");
  return () => {
    if (hydrating) {
      assign_nodes(hydrate_node, null);
      return hydrate_node;
    }
    if (node === void 0) {
      node = create_fragment_from_html(has_start ? content : "<!>" + content);
      if (!is_fragment) node = /** @type {TemplateNode} */
      get_first_child(node);
    }
    var clone = (
      /** @type {TemplateNode} */
      use_import_node || is_firefox ? document.importNode(node, true) : node.cloneNode(true)
    );
    if (is_fragment) {
      var start = (
        /** @type {TemplateNode} */
        get_first_child(clone)
      );
      var end = (
        /** @type {TemplateNode} */
        clone.lastChild
      );
      assign_nodes(start, end);
    } else {
      assign_nodes(clone, clone);
    }
    return clone;
  };
}
function from_namespace(content, flags, ns = "svg") {
  var has_start = !content.startsWith("<!>");
  var is_fragment = (flags & TEMPLATE_FRAGMENT) !== 0;
  var wrapped = `<${ns}>${has_start ? content : "<!>" + content}</${ns}>`;
  var node;
  return () => {
    if (hydrating) {
      assign_nodes(hydrate_node, null);
      return hydrate_node;
    }
    if (!node) {
      var fragment = (
        /** @type {DocumentFragment} */
        create_fragment_from_html(wrapped)
      );
      var root = (
        /** @type {Element} */
        get_first_child(fragment)
      );
      if (is_fragment) {
        node = document.createDocumentFragment();
        while (get_first_child(root)) {
          node.appendChild(
            /** @type {TemplateNode} */
            get_first_child(root)
          );
        }
      } else {
        node = /** @type {Element} */
        get_first_child(root);
      }
    }
    var clone = (
      /** @type {TemplateNode} */
      node.cloneNode(true)
    );
    if (is_fragment) {
      var start = (
        /** @type {TemplateNode} */
        get_first_child(clone)
      );
      var end = (
        /** @type {TemplateNode} */
        clone.lastChild
      );
      assign_nodes(start, end);
    } else {
      assign_nodes(clone, clone);
    }
    return clone;
  };
}
function from_svg(content, flags) {
  return from_namespace(content, flags, "svg");
}
function from_mathml(content, flags) {
  return from_namespace(content, flags, "math");
}
function fragment_from_tree(structure, ns) {
  var fragment = create_fragment();
  for (var item of structure) {
    if (typeof item === "string") {
      fragment.append(create_text(item));
      continue;
    }
    if (item === void 0 || item[0][0] === "/") {
      fragment.append(create_comment(item ? item[0].slice(3) : ""));
      continue;
    }
    const [name, attributes, ...children] = item;
    const namespace = name === "svg" ? NAMESPACE_SVG : name === "math" ? NAMESPACE_MATHML : ns;
    var element = create_element(name, namespace, attributes == null ? void 0 : attributes.is);
    for (var key in attributes) {
      set_attribute(element, key, attributes[key]);
    }
    if (children.length > 0) {
      var target = element.nodeName === TEMPLATE_TAG ? (
        /** @type {HTMLTemplateElement} */
        element.content
      ) : element;
      target.append(
        fragment_from_tree(children, element.nodeName === "foreignObject" ? void 0 : namespace)
      );
    }
    fragment.append(element);
  }
  return fragment;
}
function from_tree(structure, flags) {
  var is_fragment = (flags & TEMPLATE_FRAGMENT) !== 0;
  var use_import_node = (flags & TEMPLATE_USE_IMPORT_NODE) !== 0;
  var node;
  return () => {
    if (hydrating) {
      assign_nodes(hydrate_node, null);
      return hydrate_node;
    }
    if (node === void 0) {
      const ns = (flags & TEMPLATE_USE_SVG) !== 0 ? NAMESPACE_SVG : (flags & TEMPLATE_USE_MATHML) !== 0 ? NAMESPACE_MATHML : void 0;
      node = fragment_from_tree(structure, ns);
      if (!is_fragment) node = /** @type {TemplateNode} */
      get_first_child(node);
    }
    var clone = (
      /** @type {TemplateNode} */
      use_import_node || is_firefox ? document.importNode(node, true) : node.cloneNode(true)
    );
    if (is_fragment) {
      var start = (
        /** @type {TemplateNode} */
        get_first_child(clone)
      );
      var end = (
        /** @type {TemplateNode} */
        clone.lastChild
      );
      assign_nodes(start, end);
    } else {
      assign_nodes(clone, clone);
    }
    return clone;
  };
}
function with_script(fn) {
  return () => run_scripts(fn());
}
function run_scripts(node) {
  if (hydrating) return node;
  const is_fragment = node.nodeType === DOCUMENT_FRAGMENT_NODE;
  const scripts = (
    /** @type {HTMLElement} */
    node.nodeName === SCRIPT_TAG ? [
      /** @type {HTMLScriptElement} */
      node
    ] : node.querySelectorAll("script")
  );
  const effect = (
    /** @type {Effect & { nodes: EffectNodes }} */
    active_effect
  );
  for (const script of scripts) {
    const clone = create_element("script");
    for (var attribute of script.attributes) {
      clone.setAttribute(attribute.name, attribute.value);
    }
    clone.textContent = script.textContent;
    if (is_fragment ? node.firstChild === script : node === script) {
      effect.nodes.start = clone;
    }
    if (is_fragment ? node.lastChild === script : node === script) {
      effect.nodes.end = clone;
    }
    script.replaceWith(clone);
  }
  return node;
}
function text(value = "") {
  if (!hydrating) {
    var t = create_text(value + "");
    assign_nodes(t, t);
    return t;
  }
  var node = hydrate_node;
  if (node.nodeType !== TEXT_NODE) {
    node.before(node = create_text());
    set_hydrate_node(node);
  } else {
    merge_text_nodes(
      /** @type {Text} */
      node
    );
  }
  assign_nodes(node, node);
  return node;
}
function comment() {
  if (hydrating) {
    assign_nodes(hydrate_node, null);
    return hydrate_node;
  }
  var frag = document.createDocumentFragment();
  var start = document.createComment("");
  var anchor = create_text();
  frag.append(start, anchor);
  assign_nodes(start, anchor);
  return frag;
}
function append(anchor, dom) {
  if (hydrating) {
    var effect = (
      /** @type {Effect & { nodes: EffectNodes }} */
      active_effect
    );
    if ((effect.f & REACTION_RAN) === 0 || effect.nodes.end === null) {
      effect.nodes.end = hydrate_node;
    }
    hydrate_next();
    return;
  }
  if (anchor === null) {
    return;
  }
  anchor.before(
    /** @type {Node} */
    dom
  );
}
function props_id() {
  var _a2, _b;
  if (hydrating && hydrate_node && hydrate_node.nodeType === COMMENT_NODE && ((_a2 = hydrate_node.textContent) == null ? void 0 : _a2.startsWith(`$`))) {
    const id = hydrate_node.textContent.substring(1);
    hydrate_next();
    return id;
  }
  (_b = window.__svelte ?? (window.__svelte = {})).uid ?? (_b.uid = 1);
  return `c${window.__svelte.uid++}`;
}

// node_modules/svelte/src/utils.js
var regex_return_characters = /\r/g;
function hash(str) {
  str = str.replace(regex_return_characters, "");
  let hash2 = 5381;
  let i = str.length;
  while (i--) hash2 = (hash2 << 5) - hash2 ^ str.charCodeAt(i);
  return (hash2 >>> 0).toString(36);
}
var VOID_ELEMENT_NAMES = [
  "area",
  "base",
  "br",
  "col",
  "command",
  "embed",
  "hr",
  "img",
  "input",
  "keygen",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr"
];
function is_void(name) {
  return VOID_ELEMENT_NAMES.includes(name) || name.toLowerCase() === "!doctype";
}
function is_capture_event(name) {
  return name.endsWith("capture") && name !== "gotpointercapture" && name !== "lostpointercapture";
}
var DELEGATED_EVENTS = [
  "beforeinput",
  "click",
  "change",
  "dblclick",
  "contextmenu",
  "focusin",
  "focusout",
  "input",
  "keydown",
  "keyup",
  "mousedown",
  "mousemove",
  "mouseout",
  "mouseover",
  "mouseup",
  "pointerdown",
  "pointermove",
  "pointerout",
  "pointerover",
  "pointerup",
  "touchend",
  "touchmove",
  "touchstart"
];
function can_delegate_event(event_name) {
  return DELEGATED_EVENTS.includes(event_name);
}
var DOM_BOOLEAN_ATTRIBUTES = [
  "allowfullscreen",
  "async",
  "autofocus",
  "autoplay",
  "checked",
  "controls",
  "default",
  "disabled",
  "formnovalidate",
  "indeterminate",
  "inert",
  "ismap",
  "loop",
  "multiple",
  "muted",
  "nomodule",
  "novalidate",
  "open",
  "playsinline",
  "readonly",
  "required",
  "reversed",
  "seamless",
  "selected",
  "webkitdirectory",
  "defer",
  "disablepictureinpicture",
  "disableremoteplayback"
];
var ATTRIBUTE_ALIASES = {
  // no `class: 'className'` because we handle that separately
  formnovalidate: "formNoValidate",
  ismap: "isMap",
  nomodule: "noModule",
  playsinline: "playsInline",
  readonly: "readOnly",
  defaultvalue: "defaultValue",
  defaultchecked: "defaultChecked",
  srcobject: "srcObject",
  novalidate: "noValidate",
  allowfullscreen: "allowFullscreen",
  disablepictureinpicture: "disablePictureInPicture",
  disableremoteplayback: "disableRemotePlayback"
};
function normalize_attribute(name) {
  name = name.toLowerCase();
  return ATTRIBUTE_ALIASES[name] ?? name;
}
var DOM_PROPERTIES = [
  ...DOM_BOOLEAN_ATTRIBUTES,
  "formNoValidate",
  "isMap",
  "noModule",
  "playsInline",
  "readOnly",
  "value",
  "volume",
  "defaultValue",
  "defaultChecked",
  "srcObject",
  "noValidate",
  "allowFullscreen",
  "disablePictureInPicture",
  "disableRemotePlayback"
];
var PASSIVE_EVENTS = ["touchstart", "touchmove"];
function is_passive_event(name) {
  return PASSIVE_EVENTS.includes(name);
}
var STATE_CREATION_RUNES = (
  /** @type {const} */
  [
    "$state",
    "$state.raw",
    "$derived",
    "$derived.by"
  ]
);
var RUNES = (
  /** @type {const} */
  [
    ...STATE_CREATION_RUNES,
    "$state.eager",
    "$state.snapshot",
    "$props",
    "$props.id",
    "$bindable",
    "$effect",
    "$effect.pre",
    "$effect.tracking",
    "$effect.root",
    "$effect.pending",
    "$inspect",
    "$inspect().with",
    "$inspect.trace",
    "$host"
  ]
);
var RAW_TEXT_ELEMENTS = (
  /** @type {const} */
  ["textarea", "script", "style", "title"]
);
function is_raw_text_element(name) {
  return RAW_TEXT_ELEMENTS.includes(
    /** @type {typeof RAW_TEXT_ELEMENTS[number]} */
    name
  );
}
function sanitize_location(location) {
  return (
    /** @type {T} */
    location == null ? void 0 : location.replace(/\//g, "/​")
  );
}

// node_modules/svelte/src/internal/client/render.js
var should_intro = true;
function set_should_intro(value) {
  should_intro = value;
}
function set_text(text2, value) {
  var _a2;
  var str = value == null ? "" : typeof value === "object" ? `${value}` : value;
  if (str !== /** @type {any} */
  (text2[_a2 = TEXT_CACHE] ?? (text2[_a2] = text2.nodeValue))) {
    text2[TEXT_CACHE] = str;
    text2.nodeValue = `${str}`;
  }
}
function mount(component, options) {
  return _mount(component, options);
}
function hydrate(component, options) {
  init_operations();
  options.intro = options.intro ?? false;
  const target = options.target;
  const was_hydrating = hydrating;
  const previous_hydrate_node = hydrate_node;
  try {
    var anchor = get_first_child(target);
    while (anchor && (anchor.nodeType !== COMMENT_NODE || /** @type {Comment} */
    anchor.data !== HYDRATION_START)) {
      anchor = get_next_sibling(anchor);
    }
    if (!anchor) {
      throw HYDRATION_ERROR;
    }
    set_hydrating(true);
    set_hydrate_node(
      /** @type {Comment} */
      anchor
    );
    const instance = _mount(component, { ...options, anchor });
    set_hydrating(false);
    return (
      /**  @type {Exports} */
      instance
    );
  } catch (error) {
    if (error instanceof Error && error.message.split("\n").some((line) => line.startsWith("https://svelte.dev/e/"))) {
      throw error;
    }
    if (error !== HYDRATION_ERROR) {
      console.warn("Failed to hydrate: ", error);
    }
    if (options.recover === false) {
      hydration_failed();
    }
    init_operations();
    clear_text_content(target);
    set_hydrating(false);
    return mount(component, options);
  } finally {
    set_hydrating(was_hydrating);
    set_hydrate_node(previous_hydrate_node);
  }
}
var listeners = /* @__PURE__ */ new Map();
function _mount(Component, { target, anchor, props = {}, events, context, intro = true, transformError }) {
  init_operations();
  var component = void 0;
  var unmount2 = component_root(() => {
    var anchor_node = anchor ?? target.appendChild(create_text());
    boundary(
      /** @type {TemplateNode} */
      anchor_node,
      {
        pending: () => {
        }
      },
      (anchor_node2) => {
        push({});
        var ctx = (
          /** @type {ComponentContext} */
          component_context
        );
        if (context) ctx.c = context;
        if (events) {
          props.$$events = events;
        }
        if (hydrating) {
          assign_nodes(
            /** @type {TemplateNode} */
            anchor_node2,
            null
          );
        }
        should_intro = intro;
        component = Component(anchor_node2, props) || {};
        should_intro = true;
        if (hydrating) {
          active_effect.nodes.end = hydrate_node;
          if (hydrate_node === null || hydrate_node.nodeType !== COMMENT_NODE || /** @type {Comment} */
          hydrate_node.data !== HYDRATION_END) {
            hydration_mismatch();
            throw HYDRATION_ERROR;
          }
        }
        pop();
      },
      transformError
    );
    var registered_events = /* @__PURE__ */ new Set();
    var event_handle = (events2) => {
      for (var i = 0; i < events2.length; i++) {
        var event_name = events2[i];
        if (registered_events.has(event_name)) continue;
        registered_events.add(event_name);
        var passive2 = is_passive_event(event_name);
        for (const node of [target, document]) {
          var counts = listeners.get(node);
          if (counts === void 0) {
            counts = /* @__PURE__ */ new Map();
            listeners.set(node, counts);
          }
          var count = counts.get(event_name);
          if (count === void 0) {
            node.addEventListener(event_name, handle_event_propagation, { passive: passive2 });
            counts.set(event_name, 1);
          } else {
            counts.set(event_name, count + 1);
          }
        }
      }
    };
    event_handle(array_from(all_registered_events));
    root_event_handles.add(event_handle);
    return () => {
      var _a2;
      for (var event_name of registered_events) {
        for (const node of [target, document]) {
          var counts = (
            /** @type {Map<string, number>} */
            listeners.get(node)
          );
          var count = (
            /** @type {number} */
            counts.get(event_name)
          );
          if (--count == 0) {
            node.removeEventListener(event_name, handle_event_propagation);
            counts.delete(event_name);
            if (counts.size === 0) {
              listeners.delete(node);
            }
          } else {
            counts.set(event_name, count);
          }
        }
      }
      root_event_handles.delete(event_handle);
      if (anchor_node !== anchor) {
        (_a2 = anchor_node.parentNode) == null ? void 0 : _a2.removeChild(anchor_node);
      }
    };
  });
  mounted_components.set(component, unmount2);
  return component;
}
var mounted_components = /* @__PURE__ */ new WeakMap();
function unmount(component, options) {
  const fn = mounted_components.get(component);
  if (fn) {
    mounted_components.delete(component);
    return fn(options);
  }
  if (true_default) {
    if (STATE_SYMBOL in component) {
      state_proxy_unmount();
    } else {
      lifecycle_double_unmount();
    }
  }
  return Promise.resolve();
}

// node_modules/svelte/src/internal/client/dom/legacy/event-modifiers.js
function trusted(fn) {
  return function(...args) {
    var event = (
      /** @type {Event} */
      args[0]
    );
    if (event.isTrusted) {
      fn == null ? void 0 : fn.apply(this, args);
    }
  };
}
function self(fn) {
  return function(...args) {
    var event = (
      /** @type {Event} */
      args[0]
    );
    if (event.target === this) {
      fn == null ? void 0 : fn.apply(this, args);
    }
  };
}
function stopPropagation(fn) {
  return function(...args) {
    var event = (
      /** @type {Event} */
      args[0]
    );
    event.stopPropagation();
    return fn == null ? void 0 : fn.apply(this, args);
  };
}
function once(fn) {
  var ran = false;
  return function(...args) {
    if (ran) return;
    ran = true;
    return fn == null ? void 0 : fn.apply(this, args);
  };
}
function stopImmediatePropagation(fn) {
  return function(...args) {
    var event = (
      /** @type {Event} */
      args[0]
    );
    event.stopImmediatePropagation();
    return fn == null ? void 0 : fn.apply(this, args);
  };
}
function preventDefault(fn) {
  return function(...args) {
    var event = (
      /** @type {Event} */
      args[0]
    );
    event.preventDefault();
    return fn == null ? void 0 : fn.apply(this, args);
  };
}
function passive(node, [event, handler]) {
  user_pre_effect(() => {
    return on(node, event, handler() ?? noop, {
      passive: true
    });
  });
}
function nonpassive(node, [event, handler]) {
  user_pre_effect(() => {
    return on(node, event, handler() ?? noop, {
      passive: false
    });
  });
}

// node_modules/svelte/src/legacy/legacy-client.js
function createClassComponent(options) {
  return new Svelte4Component(options);
}
function asClassComponent(component) {
  return class extends Svelte4Component {
    /** @param {any} options */
    constructor(options) {
      super({
        component,
        ...options
      });
    }
  };
}
var _events, _instance;
var Svelte4Component = class {
  /**
   * @param {ComponentConstructorOptions & {
   *  component: any;
   * }} options
   */
  constructor(options) {
    /** @type {any} */
    __privateAdd(this, _events);
    /** @type {Record<string, any>} */
    __privateAdd(this, _instance);
    var _a2;
    var sources = /* @__PURE__ */ new Map();
    var add_source = (key, value) => {
      var s = mutable_source(value, false, false);
      sources.set(key, s);
      return s;
    };
    const props = new Proxy(
      { ...options.props || {}, $$events: {} },
      {
        get(target, prop) {
          return get(sources.get(prop) ?? add_source(prop, Reflect.get(target, prop)));
        },
        has(target, prop) {
          if (prop === LEGACY_PROPS) return true;
          get(sources.get(prop) ?? add_source(prop, Reflect.get(target, prop)));
          return Reflect.has(target, prop);
        },
        set(target, prop, value) {
          set(sources.get(prop) ?? add_source(prop, value), value);
          return Reflect.set(target, prop, value);
        }
      }
    );
    __privateSet(this, _instance, (options.hydrate ? hydrate : mount)(options.component, {
      target: options.target,
      anchor: options.anchor,
      props,
      context: options.context,
      intro: options.intro ?? false,
      recover: options.recover,
      transformError: options.transformError
    }));
    if (!async_mode_flag && (!((_a2 = options == null ? void 0 : options.props) == null ? void 0 : _a2.$$host) || options.sync === false)) {
      flushSync();
    }
    __privateSet(this, _events, props.$$events);
    for (const key of Object.keys(__privateGet(this, _instance))) {
      if (key === "$set" || key === "$destroy" || key === "$on") continue;
      define_property(this, key, {
        get() {
          return __privateGet(this, _instance)[key];
        },
        /** @param {any} value */
        set(value) {
          __privateGet(this, _instance)[key] = value;
        },
        enumerable: true
      });
    }
    __privateGet(this, _instance).$set = /** @param {Record<string, any>} next */
    (next) => {
      Object.assign(props, next);
    };
    __privateGet(this, _instance).$destroy = () => {
      unmount(__privateGet(this, _instance));
    };
  }
  /** @param {Record<string, any>} props */
  $set(props) {
    __privateGet(this, _instance).$set(props);
  }
  /**
   * @param {string} event
   * @param {(...args: any[]) => any} callback
   * @returns {any}
   */
  $on(event, callback) {
    __privateGet(this, _events)[event] = __privateGet(this, _events)[event] || [];
    const cb = (...args) => callback.call(this, ...args);
    __privateGet(this, _events)[event].push(cb);
    return () => {
      __privateGet(this, _events)[event] = __privateGet(this, _events)[event].filter(
        /** @param {any} fn */
        (fn) => fn !== cb
      );
    };
  }
  $destroy() {
    __privateGet(this, _instance).$destroy();
  }
};
_events = new WeakMap();
_instance = new WeakMap();
function run(fn) {
  user_pre_effect(() => {
    var _a2;
    fn();
    var effect = (
      /** @type {import('#client').Effect} */
      active_effect
    );
    if ((effect.f & DIRTY) !== 0) {
      let filename = "a file (we can't know which one)";
      if (true_default) {
        filename = ((_a2 = dev_current_component_function) == null ? void 0 : _a2[FILENAME]) ?? filename;
      }
      legacy_recursive_reactive_block(filename);
      set_signal_status(effect, MAYBE_DIRTY);
    }
  });
}
function handlers(...handlers2) {
  return function(event) {
    const { stopImmediatePropagation: stopImmediatePropagation2 } = event;
    let stopped = false;
    event.stopImmediatePropagation = () => {
      stopped = true;
      stopImmediatePropagation2.call(event);
    };
    const errors = [];
    for (const handler of handlers2) {
      try {
        handler == null ? void 0 : handler.call(this, event);
      } catch (e) {
        errors.push(e);
      }
      if (stopped) {
        break;
      }
    }
    for (let error of errors) {
      queueMicrotask(() => {
        throw error;
      });
    }
  };
}
function createBubbler() {
  const active_component_context = component_context;
  if (active_component_context === null) {
    lifecycle_outside_component("createBubbler");
  }
  return (type) => (event) => {
    var _a2;
    const events = (
      /** @type {Record<string, Function | Function[]>} */
      (_a2 = active_component_context.s.$$events) == null ? void 0 : _a2[
        /** @type {any} */
        type
      ]
    );
    if (events) {
      const callbacks = is_array(events) ? events.slice() : [events];
      for (const fn of callbacks) {
        fn.call(active_component_context.x, event);
      }
      return !event.defaultPrevented;
    }
    return true;
  };
}

export {
  hash,
  is_void,
  is_capture_event,
  can_delegate_event,
  normalize_attribute,
  is_raw_text_element,
  sanitize_location,
  create_trusted_html,
  create_fragment_from_html,
  assign_nodes,
  from_html,
  from_svg,
  from_mathml,
  from_tree,
  with_script,
  text,
  comment,
  append,
  props_id,
  should_intro,
  set_should_intro,
  set_text,
  mount,
  hydrate,
  unmount,
  trusted,
  self,
  stopPropagation,
  once,
  stopImmediatePropagation,
  preventDefault,
  passive,
  nonpassive,
  createClassComponent,
  asClassComponent,
  run,
  handlers,
  createBubbler
};
//# sourceMappingURL=chunk-764NIRZ7.js.map
