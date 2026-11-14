(ns sa.ingenious.pdf
  "PDF text, layout and tables from Clojure.

  Open a document with `open` and close it with `with-open`:

      (with-open [doc (pdf/open \"report.pdf\")]
        (into [] (map :text) (pdf/pages doc)))

  Sources (`pages`, `raw-pages`, `page-texts`, `page-summaries`, `tables`, `table-rows`) are
  reducible, not seqs. They read one page at a time inside `reduce`, `transduce`, `into`, `run!`
  or `eduction`, and free their native cursor when the reduction ends, however it ends.

  Every source takes cursor options that narrow the document's pages, never widen them:
    :pages       the 1-based page numbers to keep ([] keeps none)
    :skip        how many of those to pass over first
    :max-pages   the most pages to read
    :read-ahead  how many pages may run ahead of the reader (default 50); it also caps the
                 parallelism and the pages that wait finished behind a slow one
  A first read with {:max-pages 2} and a second with {:skip 2} read every page once.

  Layout and raw pages are views: read-only maps over the page's columns, so a page of 4,000
  characters costs a few arrays, not 4,000 maps. A view keeps its whole page alive. To keep data
  past the page, copy it with `realize` (or `clojure.datafy/datafy`).

  Failures throw `ex-info` with:
    :cognitect.anomalies/category  e.g. :cognitect.anomalies/not-found
    :cognitect.anomalies/message   the message
    ::error                        e.g. :io-not-found, :invalid-argument
    ::code                         the native status, when there is one
    ::key                          the option at fault, when there is one

  The JVM must allow native access: `--enable-native-access=ALL-UNNAMED`."
  (:require [clojure.core.protocols :as protocols]
            [clojure.string :as str])
  (:import [clojure.lang Associative Counted IFn IHashEq IKVReduce IKeywordLookup ILookup
            ILookupThunk IMeta IObj IPersistentCollection IPersistentMap IReduce IReduceInit
            IPersistentVector Indexed MapEntry MapEquivalence Murmur3 PersistentArrayMap Seqable
            Sequential]
           [java.io File InputStream]
           [java.nio.file Path]
           [java.util Collection List]
           [sa.ingenious.pdf BoundingBox Cursor CursorOptions DigitStyle Document Glyph LayoutOptions
            LayoutPage
            Metadata MetadataEntry Options PageSummary PageTableRows PageText PageTextBox
            PageTextLine PdfException RawPage RawTable RawTableBoundingBox RawTableCell Table
            TableCell TableOptions TableStrategy TextBox TextLine TextPage]))

(set! *warn-on-reflection* true)

;;;; Errors

(def ^:private failures
  "Each Java failure, by class name, as its ::error and anomaly category."
  {"InvalidPath"        [:invalid-path         :cognitect.anomalies/incorrect]
   "InvalidArgument"    [:invalid-argument     :cognitect.anomalies/incorrect]
   "IoNotFound"         [:io-not-found         :cognitect.anomalies/not-found]
   "IoPermissionDenied" [:io-permission-denied :cognitect.anomalies/forbidden]
   "IoError"            [:io-error             :cognitect.anomalies/fault]
   "SyntaxError"        [:syntax-error         :cognitect.anomalies/incorrect]
   "EncryptionError"    [:encryption-error     :cognitect.anomalies/forbidden]
   "MalformedPdf"       [:malformed-pdf        :cognitect.anomalies/incorrect]
   "DecodeError"        [:decode-error         :cognitect.anomalies/incorrect]
   "Cancelled"          [:cancelled            :cognitect.anomalies/interrupted]
   "RuntimeError"       [:runtime-error        :cognitect.anomalies/fault]
   "Unsupported"        [:unsupported          :cognitect.anomalies/unsupported]
   "NativeError"        [:native-error         :cognitect.anomalies/fault]})

(defn- anomaly [error category message]
  {::error                       error
   :cognitect.anomalies/category category
   :cognitect.anomalies/message  message})

(defn- invalid!
  "Throw for a bad argument from Clojure, before any native call."
  [key message value]
  (throw (ex-info message (assoc (anomaly :invalid-argument :cognitect.anomalies/incorrect message)
                                 ::key key
                                 ::value value))))

(defn- option-key
  "A native option name (`snap_tolerance`) as its Clojure key (`:snap-tolerance`)."
  [field]
  (keyword (str/replace field "_" "-")))

(defn- translate
  "The ex-info for a Bolivar failure, or nil for any other exception."
  [^Throwable e]
  (condp instance? e
    PdfException
    (let [^PdfException e e
          [error category] (failures (.getSimpleName (class e))
                                     [:native-error :cognitect.anomalies/fault])]
      (ex-info (.getMessage e)
               (cond-> (assoc (anomaly error category (.getMessage e)) ::code (.code e))
                 (.field e) (assoc ::key (option-key (.field e))))
               e))

    ;; A closed document, or a jar and native library from different builds.
    IllegalStateException
    (ex-info (.getMessage e) (anomaly :illegal-state :cognitect.anomalies/conflict (.getMessage e)) e)

    nil))

(defmacro ^:private bolivar
  "Run a Bolivar call, rethrowing its failures as ex-info. Wrap Bolivar calls only, so that a
  caller's own exceptions pass through unchanged."
  [& body]
  `(try
     ~@body
     (catch clojure.lang.ExceptionInfo e# (throw e#))
     (catch RuntimeException e# (throw (or (translate e#) e#)))))

;;;; Options
;;
;; Each option map has a spec: key -> type. `conform` checks keys and types and drops nil values
;; (nil leaves an option unset). Native code checks the values themselves and names the option
;; at fault, which arrives as ::key.

(declare layout-options)

(defn- int32? [v]
  (and (integer? v) (<= Integer/MIN_VALUE v Integer/MAX_VALUE)))

(defn- coerce
  "`v` as the Java value an option of type `type` takes."
  [type key v]
  (letfn [(want [what] (invalid! key (str key " must be " what) v))]
    (case type
      :string   (if (string? v) v (want "a string"))
      :boolean  (if (boolean? v) v (want "true or false"))
      :int      (if (int32? v) (Integer/valueOf (int v)) (want "an integer"))
      :number   (if (number? v) (Double/valueOf (double v)) (want "a number"))
      :pages    (if (and (sequential? v) (every? int32? v))
                  (mapv #(Integer/valueOf (int %)) v)
                  (want "a sequence of 1-based page numbers"))
      :numbers  (if (and (sequential? v) (every? number? v))
                  (mapv #(Double/valueOf (double %)) v)
                  (want "a sequence of numbers"))
      :box      (if (and (sequential? v) (= 4 (count v)) (every? number? v))
                  (let [[x0 y0 x1 y1] (map double v)] (BoundingBox. x0 y0 x1 y1))
                  (want "[x0 y0 x1 y1]"))
      :strategy (if (or (keyword? v) (string? v))
                  (try (TableStrategy/parse (str/replace (name v) "-" "_"))
                       (catch IllegalArgumentException e (invalid! key (.getMessage e) v)))
                  (want "a keyword such as :lines"))
      :layout   (layout-options v))))

(defn- conform
  "Check map `m` against `spec`. The result holds Java values, and no nils."
  [spec what m]
  (when-not (or (nil? m) (map? m))
    (invalid! nil (str what " must be a map") m))
  (reduce-kv (fn [out k v]
               (let [type (or (spec k) (invalid! k (str "Unknown " what " " k) m))]
                 (cond-> out (some? v) (assoc k (coerce type k v)))))
             {}
             m))

(def ^:private layout-spec
  {:line-overlap    :number
   :char-margin     :number
   :line-margin     :number
   :word-margin     :number
   :boxes-flow      :number
   :detect-vertical :boolean
   :all-texts       :boolean})

(defn- layout-options ^LayoutOptions [m]
  (let [o (conform layout-spec "layout option" m)]
    (LayoutOptions. (:line-overlap o) (:char-margin o) (:line-margin o) (:word-margin o)
                    (:boxes-flow o)
                    ;; An explicit nil :boxes-flow turns box ordering off, as in pdfminer.
                    (and (contains? m :boxes-flow) (nil? (:boxes-flow m)))
                    (:detect-vertical o) (:all-texts o))))

(def ^:private document-spec
  {:password  :string
   :pages     :pages
   :max-pages :int
   :caching   :boolean
   :bidi      :boolean
   :layout    :layout})

(defn- options ^Options [m]
  (let [o (conform document-spec "option" m)]
    (Options. (:password o) (:pages o) (:max-pages o) (:caching o) (:bidi o) (:layout o))))

(def ^:private table-spec
  {:vertical-strategy         :strategy
   :horizontal-strategy       :strategy
   :snap-tolerance            :number
   :snap-x-tolerance          :number
   :snap-y-tolerance          :number
   :join-tolerance            :number
   :join-x-tolerance          :number
   :join-y-tolerance          :number
   :intersection-tolerance    :number
   :intersection-x-tolerance  :number
   :intersection-y-tolerance  :number
   :explicit-vertical-lines   :numbers
   :explicit-horizontal-lines :numbers
   :crop                      :box
   :first-page-crop           :box})

(defn- table-options ^TableOptions [m]
  (let [o (conform table-spec "table option" m)]
    (TableOptions. (:vertical-strategy o) (:horizontal-strategy o)
                   (:snap-tolerance o) (:snap-x-tolerance o) (:snap-y-tolerance o)
                   (:join-tolerance o) (:join-x-tolerance o) (:join-y-tolerance o)
                   (:intersection-tolerance o) (:intersection-x-tolerance o)
                   (:intersection-y-tolerance o)
                   (:explicit-vertical-lines o) (:explicit-horizontal-lines o)
                   (:crop o) (:first-page-crop o)
                   ;; :max-pages is a cursor option now, for every source.
                   nil)))

(def ^:private cursor-spec
  {:pages      :pages
   :skip       :int
   :max-pages  :int
   :read-ahead :int})

(defn- cursor-options ^CursorOptions [m]
  (let [o (conform cursor-spec "cursor option" m)]
    (CursorOptions. (:pages o) (:skip o) (:max-pages o) (:read-ahead o))))

(defn- split-options
  "A source's opts map as [table options, cursor options], each checked against its spec."
  [m]
  (when-not (or (nil? m) (map? m))
    (invalid! nil "table option must be a map" m))
  [(table-options (apply dissoc m (keys cursor-spec)))
   (cursor-options (select-keys m (keys cursor-spec)))])

;;;; Views
;;
;; A view is a read-only map over one object of a page: a glyph, line, box or the page itself.
;; Its kind lists the keys and how to read each one. `ctx` is what a kind needs besides the
;; object: raw kinds read graphics state from their RawPage. A view equals, and hashes like,
;; the plain map `realize` makes of it.

;; `fns` holds each key's getter in key order; `getters` maps keys to the same functions.
(deftype Kind [name ks fns getters realize])

(defmacro ^:private defkind
  "Define a view kind. `args` binds the object and, optionally, the context; each key is read by
  its expression. Leading options: `:extends kind` starts from that kind's keys and getters (its
  keys first), and `:realize expr` replaces the default copy of every entry."
  [kind-name args & body]
  (let [options   (into {} (map vec) (take-while (comp #{:extends :realize} first)
                                                 (partition 2 body)))
        pairs     (drop (count options) (partition 2 body))
        [obj ctx] (if (= 2 (count args)) args [(first args) '_])
        parent    (:extends options)]
    `(def ~(vary-meta kind-name assoc :private true)
       (let [ks#  (into ~(if parent `(.-ks ~(with-meta parent {:tag `Kind})) []) ~(mapv first pairs))
             fns# (into ~(if parent `(.-fns ~(with-meta parent {:tag `Kind})) [])
                        ~(mapv (fn [[_ expr]] `(fn [~obj ~ctx] ~expr)) pairs))]
         (Kind. '~kind-name ks# fns# (zipmap ks# fns#)
                ~(when-let [realize (:realize options)] `(fn [~obj ~ctx] ~realize)))))))

(defprotocol ^:private Realize
  (-realize [x]))

(defn view?
  "Whether `x` is a view (see `realize`)."
  [x]
  (instance? sa.ingenious.pdf.Realize x))

(defn realize
  "Copy a view, and every view inside it, into plain maps and vectors that keep nothing of the
  page alive. Other values come back unchanged."
  [x]
  (if (view? x) (-realize x) x))

(defn- entries
  "A view's entries, in key order."
  ^clojure.lang.PersistentVector [^Kind kind obj ctx]
  (let [^List fns (.-fns kind)]
    (into [] (map-indexed (fn [i k] (MapEntry/create k ((.get fns i) obj ctx)))) (.-ks kind))))

(defmacro ^:private flat-map
  "A map of the given keys and values, stored flat as Clojure stores small maps: one array, no
  hashing. Lookups scan it, which beats a hash trie at these sizes."
  [& kvs]
  (let [arr (gensym "arr")]
    `(let [~arr (object-array ~(count kvs))]
       ~@(map-indexed (fn [i form] `(aset ~arr ~i (clojure.lang.RT/box ~form))) kvs)
       (PersistentArrayMap. ~arr))))

(defn- copy-entries
  "A plain map of a view's entries, each realized, in key order; a key in `overrides` takes the
  value given there instead, as is."
  [^Kind kind obj ctx overrides]
  (let [^List ks  (.-ks kind)
        ^List fns (.-fns kind)
        n         (.size ks)
        arr       (object-array (* 2 n))]
    (dotimes [i n]
      (let [k (.get ks i)]
        (aset arr (* 2 i) k)
        (aset arr (inc (* 2 i))
              (if (contains? overrides k) (get overrides k) (realize ((.get fns i) obj ctx))))))
    (PersistentArrayMap. arr)))

(defn- realize-as
  "A plain map of the view of `kind` over `obj`."
  [^Kind kind obj ctx]
  (if-let [realize-kind (.-realize kind)]
    (realize-kind obj ctx)
    (copy-entries kind obj ctx nil)))

(defn- map-equiv
  "Equality with any java.util.Map, as Clojure's maps define it."
  [m o]
  (and (instance? java.util.Map o)
       (let [^java.util.Map o o]
         (and (= (count m) (.size o))
              (every? (fn [[k v]] (and (.containsKey o k) (= v (.get o k)))) m)))))

(deftype View [^Kind kind obj ctx metadata]
  ILookup
  (valAt [this k] (.valAt this k nil))
  (valAt [_ k not-found]
    (if-let [getter (get (.-getters kind) k)] (getter obj ctx) not-found))

  IKeywordLookup
  (getLookupThunk [_ k]
    ;; `(:text glyph)` finds the getter for :text once per call site, for views of this kind.
    (when-let [getter (get (.-getters kind) k)]
      (reify ILookupThunk
        (get [thunk target]
          (if (and (instance? View target) (identical? kind (.-kind ^View target)))
            (getter (.-obj ^View target) (.-ctx ^View target))
            thunk)))))

  IFn
  (invoke [this k] (.valAt this k nil))
  (invoke [this k not-found] (.valAt this k not-found))
  (applyTo [this args] (clojure.lang.AFn/applyToHelper this args))

  Counted
  (count [_] (count (.-ks kind)))

  Seqable
  (seq [_] (seq (entries kind obj ctx)))

  IKVReduce
  (kvreduce [_ f init]
    (let [^List fns (.-fns kind)]
      (reduce-kv (fn [acc i k] (f acc k ((.get fns i) obj ctx))) init (.-ks kind))))

  Associative
  (containsKey [_ k] (contains? (.-getters kind) k))
  (entryAt [this k] (when (.containsKey this k) (MapEntry/create k (.valAt this k nil))))
  (assoc [this k v] (assoc (realize this) k v))

  IPersistentMap
  (assocEx [this k v] (.assocEx ^IPersistentMap (realize this) k v))
  (without [this k] (dissoc (realize this) k))

  IPersistentCollection
  (cons [this o] (conj (realize this) o))
  (empty [_] {})
  (equiv [this o] (map-equiv this o))

  MapEquivalence

  IHashEq
  (hasheq [this] (Murmur3/hashUnordered this))

  Iterable
  (iterator [_] (.iterator (entries kind obj ctx)))

  IMeta
  (meta [_] metadata)

  IObj
  (withMeta [_ m] (View. kind obj ctx m))

  Realize
  (-realize [_] (cond-> (realize-as kind obj ctx) metadata (with-meta metadata)))

  protocols/Datafiable
  (datafy [this] (realize this))

  java.util.Map
  (size [_] (count (.-ks kind)))
  (isEmpty [_] (empty? (.-ks kind)))
  (containsValue [this v] (boolean (some #(= v (val %)) (seq this))))
  (get [this k] (.valAt this k nil))
  (keySet [_] (java.util.LinkedHashSet. ^Collection (.-ks kind)))
  (values [this] (mapv val (seq this)))
  (entrySet [_] (java.util.LinkedHashSet. ^Collection (entries kind obj ctx)))
  (put [_ _ _] (throw (UnsupportedOperationException.)))
  (remove [_ _] (throw (UnsupportedOperationException.)))
  (putAll [_ _] (throw (UnsupportedOperationException.)))
  (clear [_] (throw (UnsupportedOperationException.)))

  Object
  (equals [this o] (clojure.lang.APersistentMap/mapEquals this o))
  (hashCode [this] (clojure.lang.APersistentMap/mapHash this))
  (toString [this] (pr-str this)))

(defn- index-of
  "`k` as an index into `n` items, or nil, as Clojure's vectors read keys."
  [k ^long n]
  (when (clojure.lang.Util/isInteger k)
    (let [i (long k)]
      (when (< -1 i n) i))))

(deftype Views [^Kind kind ^List xs ctx metadata]
  ;; A read-only vector of views of `kind`, one per element of the Java list `xs`. It reads as
  ;; a vector does (get, get-in, contains?, subvec, peek, rseq); changing it realizes it.
  Counted
  (count [_] (.size xs))

  Indexed
  (nth [_ i] (View. kind (.get xs i) ctx nil))
  (nth [this i not-found] (if (< -1 i (.size xs)) (.nth this i) not-found))

  ILookup
  (valAt [this k] (.valAt this k nil))
  (valAt [this k not-found]
    (if-some [i (index-of k (.size xs))] (.nth this (int i)) not-found))

  IFn
  (invoke [this i] (.nth this (int i)))
  (applyTo [this args] (clojure.lang.AFn/applyToHelper this args))

  IPersistentVector
  (length [_] (.size xs))
  (assocN [this i v] (assoc (realize this) i v))
  (containsKey [_ k] (some? (index-of k (.size xs))))
  (entryAt [this k]
    (when-some [i (index-of k (.size xs))] (MapEntry/create k (.nth this (int i)))))
  (assoc [this k v] (assoc (realize this) k v))
  (peek [this] (when-not (.isEmpty xs) (.nth this (dec (.size xs)))))
  (pop [this] (pop (realize this)))
  (rseq [this]
    (when-not (.isEmpty xs)
      (map #(.nth this (int %)) (range (dec (.size xs)) -1 -1))))

  Sequential

  Seqable
  (seq [_] (seq (mapv #(View. kind % ctx nil) xs)))

  IReduceInit
  (reduce [_ f init]
    (reduce (fn [acc x] (f acc (View. kind x ctx nil))) init xs))

  IReduce
  (reduce [this f]
    (if (.isEmpty xs) (f) (reduce f (seq this))))

  IPersistentCollection
  (cons [this o] (conj (realize this) o))
  (empty [_] [])
  (equiv [this o]
    (and (or (sequential? o) (instance? List o))
         (= (.size xs) (count o))
         (= (seq this) (seq o))))

  IHashEq
  (hasheq [this] (Murmur3/hashOrdered this))

  Iterable
  (iterator [_] (.iterator ^Iterable (mapv #(View. kind % ctx nil) xs)))

  IMeta
  (meta [_] metadata)

  IObj
  (withMeta [_ m] (Views. kind xs ctx m))

  Realize
  (-realize [this] (cond-> (into [] (map realize) this) metadata (with-meta metadata)))

  protocols/Datafiable
  (datafy [this] (realize this))

  Object
  (equals [this o] (.equiv this o))
  (hashCode [this] (.hashCode ^Object (realize this)))
  (toString [this] (pr-str this)))

(defmethod print-method View [v ^java.io.Writer w]
  ;; In the kind's key order, as `realize` makes it.
  (print-method (apply array-map (mapcat identity v)) w))
(defmethod print-method Views [v ^java.io.Writer w] (print-method (vec v) w))

(defn- views [kind ^List xs ctx] (Views. kind xs ctx nil))

(defn- realize-page
  "A page view of `page-kind` as plain data, each glyph copied once: a line's :chars is a subvec
  of the page's :chars, and a box's :lines a subvec of the page's :lines."
  [page-kind ^TextPage page ctx glyph-kind line-kind box-kind]
  (let [chars (mapv #(realize-as glyph-kind % ctx) (.glyphs page))
        lines (mapv (fn [^TextLine l]
                      (copy-entries line-kind l ctx
                                    {:chars (subvec chars (.glyphStart l) (.glyphEnd l))}))
                    (.lines page))
        boxes (mapv (fn [^TextBox b]
                      (copy-entries box-kind b ctx
                                    {:lines (subvec lines (.lineStart b) (.lineEnd b))}))
                    (.textBoxes page))]
    (copy-entries page-kind page ctx {:text-boxes boxes :lines lines :chars chars})))

(defn- bbox [^BoundingBox b]
  {:x0 (.x0 b) :y0 (.y0 b) :x1 (.x1 b) :y1 (.y1 b)})

(defn- top-down-bbox [^RawTableBoundingBox b]
  {:x0 (.x0 b) :top (.top b) :x1 (.x1 b) :bottom (.bottom b)})

(defn- some-vec [xs] (some-> xs vec))

;;; Layout pages: boxes of lines of glyphs.

(defmacro ^:private glyph-map
  "The realized entries every glyph has, each coordinate boxed once and shared with :bbox, then
  `more` entries."
  [g & more]
  `(let [x0# (Double/valueOf (.x0 ~g))
         y0# (Double/valueOf (.y0 ~g))
         x1# (Double/valueOf (.x1 ~g))
         y1# (Double/valueOf (.y1 ~g))]
     (flat-map :text (.text ~g)
               :bbox (flat-map :x0 x0# :y0 y0# :x1 x1# :y1 y1#)
               :x0 x0# :y0 y0# :x1 x1# :y1 y1#
               :font-name (.fontName ~g)
               :size (.size ~g)
               :upright (.upright ~g)
               ~@more)))

(defkind glyph [^Glyph g]
  :realize (glyph-map g)
  :text      (.text g)
  :bbox      (bbox (.bbox g))
  :x0        (.x0 g)
  :y0        (.y0 g)
  :x1        (.x1 g)
  :y1        (.y1 g)
  :font-name (.fontName g)
  :size      (.size g)
  :upright   (.upright g))

(defkind line [^TextLine l]
  :bbox        (bbox (.bbox l))
  :orientation (.orientation l)
  :text        (.text l)
  :chars       (views glyph (.glyphs l) nil))

(defkind box [^TextBox b]
  :bbox         (bbox (.bbox b))
  :writing-mode (.writingMode b)
  :text         (.text b)
  :lines        (views line (.lines b) nil))

(declare layout-page)

(defkind layout-page [^LayoutPage p]
  :realize (realize-page layout-page p nil glyph line box)
  :page-number (.pageNumber p)
  :bbox        (bbox (.bbox p))
  :rotate      (.rotate p)
  :text        (.text p)
  :text-boxes  (views box (.textBoxes p) nil)
  :lines       (views line (.lines p) nil)
  :chars       (views glyph (.glyphs p) nil))

;;; Raw pages: the same layers, plus each glyph's graphics state, each line's raw text, the
;;; page's PDF identity and its tables. The context of every raw kind is the RawPage.

(defn- glyph-matrix [^RawPage page ^long i]
  [(.glyphMatrix page i 0) (.glyphMatrix page i 1) (.glyphMatrix page i 2)
   (.glyphMatrix page i 3) (.glyphMatrix page i 4) (.glyphMatrix page i 5)])

(defkind raw-glyph [^Glyph g ^RawPage page]
  :extends glyph
  :realize (let [i (.index g)]
             (glyph-map g
                        :advance (.glyphAdvance page i)
                        :matrix (glyph-matrix page i)
                        :marked-content-id (.glyphMarkedContentId page i)
                        :tag (.glyphTag page i)
                        :non-stroking-color-space (.glyphNonStrokingColorSpace page i)
                        :stroking-color-space (.glyphStrokingColorSpace page i)
                        :non-stroking-color (some-vec (.glyphNonStrokingColor page i))
                        :stroking-color (some-vec (.glyphStrokingColor page i))))
  :advance                  (.glyphAdvance page (.index g))
  :matrix                   (glyph-matrix page (.index g))
  :marked-content-id        (.glyphMarkedContentId page (.index g))
  :tag                      (.glyphTag page (.index g))
  :non-stroking-color-space (.glyphNonStrokingColorSpace page (.index g))
  :stroking-color-space     (.glyphStrokingColorSpace page (.index g))
  :non-stroking-color       (some-vec (.glyphNonStrokingColor page (.index g)))
  :stroking-color           (some-vec (.glyphStrokingColor page (.index g))))

(defkind raw-line [^TextLine l page]
  :bbox        (bbox (.bbox l))
  :orientation (.orientation l)
  :raw-text    (.rawText l)
  :text        (.text l)
  :chars       (views raw-glyph (.glyphs l) page))

(defkind raw-box [^TextBox b page]
  :bbox         (bbox (.bbox b))
  :writing-mode (.writingMode b)
  :text         (.text b)
  :lines        (views raw-line (.lines b) page))

(defn- cell-map [row column row-span column-span bbox text]
  {:row-index    row
   :column-index column
   :row-span     row-span
   :column-span  column-span
   :bbox         bbox
   :text         text})

(defn- raw-table [^RawTable t]
  {:bbox         (top-down-bbox (.bbox t))
   :row-count    (.rowCount t)
   :column-count (.columnCount t)
   :cells        (mapv (fn [^RawTableCell c]
                         (cell-map (.rowIndex c) (.columnIndex c) (.rowSpan c) (.columnSpan c)
                                   (top-down-bbox (.bbox c)) (.text c)))
                       (.cells t))})

(defn- page-boxes [^RawPage p]
  (let [b (.boxes p)]
    {:media (some-vec (.media b))
     :crop  (some-vec (.crop b))
     :bleed (some-vec (.bleed b))
     :trim  (some-vec (.trim b))
     :art   (some-vec (.art b))}))

(declare raw-page-kind)

(defkind raw-page-kind [^RawPage p]
  :realize (realize-page raw-page-kind p p raw-glyph raw-line raw-box)
  :page-index  (.pageIndex p)
  :page-number (.pageNumber p)
  :object-id   (.objectId p)
  :label       (.label p)
  :rotation    (.rotation p)
  :user-unit   (.userUnit p)
  :boxes       (page-boxes p)
  :layout-bbox (bbox (.layoutBbox p))
  :text        (.text p)
  :text-boxes  (views raw-box (.textBoxes p) p)
  :lines       (views raw-line (.lines p) p)
  :chars       (views raw-glyph (.glyphs p) p)
  :tables      (mapv raw-table (.tables p)))

(defn java-page
  "The Java page (`LayoutPage` or `RawPage`) a page view reads."
  [page]
  (if (and (instance? View page) (#{layout-page raw-page-kind} (.-kind ^View page)))
    (.-obj ^View page)
    (invalid! nil "java-page takes a page from pages, raw-pages or raw-page" page)))

(defn columns
  "A page's glyph columns as fresh arrays: doubles for :x0 :y0 :x1 :y1 :size (and :advance on
  raw pages), booleans for :upright, strings for :font-name and :text."
  [page]
  (let [^TextPage p (java-page page)]
    (cond-> {:x0        (.glyphX0s p)
             :y0        (.glyphY0s p)
             :x1        (.glyphX1s p)
             :y1        (.glyphY1s p)
             :size      (.glyphSizes p)
             :upright   (.glyphUprights p)
             :font-name (.glyphFontNames p)
             :text      (.glyphTexts p)}
      (instance? RawPage p) (assoc :advance (.glyphAdvances ^RawPage p)))))

;;;; Small results, as plain maps

(defn- page-text-map [^PageText p]
  {:page-number  (.pageNumber p)
   :bbox         (bbox (.bbox p))
   :text-boxes   (mapv (fn [^PageTextBox b]
                         {:bbox              (bbox (.bbox b))
                          :text              (.text b)
                          :text-without-bidi (.textWithoutBidi b)
                          :lines             (mapv (fn [^PageTextLine l]
                                                     {:bbox              (bbox (.bbox l))
                                                      :text              (.text l)
                                                      :text-without-bidi (.textWithoutBidi l)})
                                                   (.lines b))})
                       (.textBoxes p))
   :fonts        (set (.fonts p))
   :digit-styles (into #{} (map (fn [^DigitStyle s] [(.fontName s) (long (.size s))]))
                       (.digitStyles p))})

(defn- summary-map [^PageSummary s]
  {:page-number (.pageNumber s)
   :text        (.text s)
   :bbox        (bbox (.bbox s))
   :rotate      (.rotate s)})

(defn- table-map [^Table t]
  {:page-number  (.pageNumber t)
   :bbox         (bbox (.bbox t))
   :row-count    (.rowCount t)
   :column-count (.columnCount t)
   :cells        (mapv (fn [^TableCell c]
                         (cell-map (.rowIndex c) (.columnIndex c) (.rowSpan c) (.columnSpan c)
                                   (bbox (.bbox c)) (.text c)))
                       (.cells t))})

(defn- table-rows-map [^PageTableRows r]
  {:page-number   (.pageNumber r)
   :cells         (vec (.cells r))
   :row-offsets   (vec (.rowOffsets r))
   :table-offsets (vec (.tableOffsets r))})

(defn- metadata-map [^Metadata m]
  (let [v (.version m)
        p (.permissions m)]
    {:document-info         (into {} (map (fn [^MetadataEntry e] [(.key e) (.value e)]))
                                  (.documentInfo m))
     :title                 (.title m)
     :author                (.author m)
     :subject               (.subject m)
     :keywords              (.keywords m)
     :creator               (.creator m)
     :producer              (.producer m)
     :creation-date-raw     (.creationDateRaw m)
     :creation-date-iso     (.creationDateIso m)
     :modification-date-raw (.modificationDateRaw m)
     :modification-date-iso (.modificationDateIso m)
     :version               {:header    (.header v)
                             :catalog   (.catalog v)
                             :effective (.effective v)}
     :file-size-bytes       (.fileSizeBytes m)
     :page-count            (.pageCount m)
     :encrypted             (.encrypted m)
     :permissions           {:printable   (.printable p)
                             :modifiable  (.modifiable p)
                             :extractable (.extractable p)}
     :linearized            (.linearized m)
     :tagged                (.tagged m)
     :user-properties       (.userProperties m)
     :suspects              (.suspects m)
     :form                  (.form m)
     :has-javascript        (.hasJavascript m)
     :has-metadata-stream   (.hasMetadataStream m)
     :xmp-metadata          (.xmpMetadata m)}))

;;;; Documents

(defn open
  "Open a PDF; close it with `with-open`. `source` is a path string, java.nio.file.Path,
  java.io.File, java.io.InputStream (read to its end, then closed) or byte array. A file is
  memory-mapped: keep it unchanged while the document is open.

  `opts` (a nil value leaves an option unset):
    :password   the password
    :pages      the 1-based page numbers to read
    :max-pages  read at most this many pages
    :caching    cache decoded objects (default true)
    :bidi       reorder right-to-left text
    :layout     {:line-overlap :char-margin :line-margin :word-margin :detect-vertical
                 :all-texts :boxes-flow}; an explicit nil :boxes-flow turns box ordering off"
  (^Document [source] (open source nil))
  (^Document [source opts]
   (let [o (options opts)]
     (bolivar
      (condp instance? source
        String      (Document/open ^String source o)
        Path        (Document/open ^Path source o)
        File        (Document/open ^File source o)
        InputStream (Document/open ^InputStream source o)
        (if (bytes? source)
          (Document/open ^bytes source o)
          (invalid! :source "source must be a path, File, InputStream or byte array"
                    (some-> source class .getName))))))))

(defn text
  "The document's text, pages separated by form feeds."
  [^Document doc]
  (bolivar (.text doc)))

(defn extract-text
  "Open `source`, read its text, and close it."
  ([source] (extract-text source nil))
  ([source opts]
   (with-open [doc (open source opts)]
     (text doc))))

(defn metadata
  "The document's metadata, as a map."
  [^Document doc]
  (bolivar (metadata-map (.metadata doc))))

(defn page-count
  "The number of pages the document's options select."
  [^Document doc]
  (bolivar (.pageCount doc)))

(defn declared-page-count
  "The number of pages the file declares."
  [^Document doc]
  (bolivar (.declaredPageCount doc)))

(defn raw-page
  "One 1-based page, with nothing dropped, as a view."
  [^Document doc page-number]
  (let [^Integer n (coerce :int :page-number page-number)]
    (View. raw-page-kind (bolivar (.rawPage doc (.intValue n))) nil nil)))

(defn version
  "The Bolivar version."
  []
  (bolivar (Document/version)))

(defn- source
  "A reducible over the cursor `open-cursor` opens. Each reduction opens its own cursor and closes
  it at the end, so a source can be reduced again."
  [what open-cursor item]
  (letfn [(run [f init]
            (let [^Cursor cursor (bolivar (open-cursor))]
              (try
                (loop [acc init]
                  (cond
                    (reduced? acc)              @acc
                    (bolivar (.hasNext cursor)) (recur (f acc (item (bolivar (.next cursor)))))
                    :else                       acc))
                (finally (.close cursor)))))]
    (reify
      IReduceInit
      (reduce [_ f init] (run f init))

      IReduce
      (reduce [_ f]
        (let [none   (Object.)
              result (run (fn [acc x] (if (identical? acc none) x (f acc x))) none)]
          (if (identical? result none) (f) result)))

      Object
      (toString [_]
        (str "#<sa.ingenious.pdf/" what
             ": reducible, not a seq; use into, transduce, run!, reduce or eduction>")))))

(defn pages
  "The laid-out pages, as views. A page has :page-number :bbox :rotate :text :text-boxes, and all
  its :lines and :chars. Boxes hold :lines, and lines hold :chars; a char has :text :bbox :x0
  :y0 :x1 :y1 :font-name :size :upright. `opts` takes the cursor options (see the namespace)."
  ([doc] (pages doc nil))
  ([^Document doc opts]
   (let [c (cursor-options opts)]
     (source "pages" #(.pages doc c) #(View. layout-page % nil nil)))))

(defn raw-pages
  "Every page with nothing dropped, as views: the page's PDF identity and :boxes, its
  :text-boxes, :lines with :raw-text, :chars with their graphics state, and :tables in
  pdfplumber's top-down space. `opts` takes the cursor options."
  ([doc] (raw-pages doc nil))
  ([^Document doc opts]
   (let [c (cursor-options opts)]
     (source "raw-pages" #(.rawPages doc c) #(View. raw-page-kind % nil nil)))))

(defn page-texts
  "Each page's boxes and lines with :text and :text-without-bidi, its :fonts, and the
  :digit-styles [font whole-size] of its digits. `opts` takes the cursor options."
  ([doc] (page-texts doc nil))
  ([^Document doc opts]
   (let [c (cursor-options opts)]
     (source "page-texts" #(.pageTexts doc c) page-text-map))))

(defn page-summaries
  "Each page's :page-number :text :bbox :rotate. `opts` takes the cursor options."
  ([doc] (page-summaries doc nil))
  ([^Document doc opts]
   (let [c (cursor-options opts)]
     (source "page-summaries" #(.pageSummaries doc c) summary-map))))

(defn tables
  "Every table of every page, with :page-number :bbox :row-count :column-count :cells.

  `opts` uses pdfplumber's names: :vertical-strategy and :horizontal-strategy (:lines
  :lines-strict :text :explicit); :snap-tolerance, :join-tolerance, :intersection-tolerance and
  their -x- and -y- forms; :explicit-vertical-lines and :explicit-horizontal-lines; :crop and
  :first-page-crop as [x0 y0 x1 y1]. It also takes the cursor options (see the namespace)."
  ([doc] (tables doc nil))
  ([^Document doc opts]
   (let [[t c] (split-options opts)]
     (source "tables" #(.tables doc t c) table-map))))

(defn table-rows
  "Each page's raw table rows, as the pdfplumber-compatible pipeline emits them: :cells (nil for
  an empty cell), :row-offsets into the cells, :table-offsets into the rows. Takes the options of
  `tables`."
  ([doc] (table-rows doc nil))
  ([^Document doc opts]
   (let [[t c] (split-options opts)]
     (source "table-rows" #(.tableRows doc t c) table-rows-map))))
