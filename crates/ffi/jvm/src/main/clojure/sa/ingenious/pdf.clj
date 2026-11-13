(ns sa.ingenious.pdf
  "PDF text, layout and tables from Clojure.

  Open a document with `open` (use `with-open`). Page sources such as `pages` and `tables` are
  reducible: they read one page at a time from native code inside `reduce`, `transduce`, `into`,
  `run!` or `eduction`, and always free the native cursor, even on `reduced` or an exception.
  They are not seqs: `doseq`, `seq` and `first` do not apply to them.

  Layout and raw pages arrive as views: map-like values over the page's columns, so a 4,000
  character page costs a handful of arrays, not 4,000 maps. A view (or any view inside it) keeps
  its whole page alive; call `realize` to copy what you keep into plain maps.

  Failures throw `ex-info` with `:type` such as `:pdf/io-not-found` or `:pdf/invalid-argument`,
  `:code` (the native status) and `:key` (the option at fault, when one is).

  The JVM must allow native access: `--enable-native-access=ALL-UNNAMED`."
  (:require [clojure.string :as str])
  (:import [clojure.lang Associative Counted IFn IHashEq IKeywordLookup ILookup ILookupThunk
            IPersistentCollection IPersistentMap IReduce IReduceInit Indexed Keyword MapEntry
            MapEquivalence
            Murmur3 Reduced Seqable Sequential]
           [java.io File InputStream]
           [java.nio.file Path]
           [java.util Iterator NoSuchElementException]
           [sa.ingenious.pdf BoundingBox Cursor Document LayoutOptions LayoutPage Metadata
            MetadataEntry Options PageSummary PageTableRows PageText PdfException RawPage
            RawTable RawTableBoundingBox Table TableOptions TableStrategy]))

(set! *warn-on-reflection* true)

;; ---- errors --------------------------------------------------------------------------------

(defn- kebab [s]
  (-> s
      (str/replace #"([a-z0-9])([A-Z])" "$1-$2")
      (str/replace "_" "-")
      str/lower-case))

(defn- fail!
  ([message data] (throw (ex-info message data)))
  ([message data cause] (throw (ex-info message data cause))))

(defn- invalid! [key message value]
  (fail! message {:type :pdf/invalid-argument :key key :value value}))

(defn- translate
  "The ex-info for a Java failure, or nil when it is not one of ours."
  [^Throwable e]
  (cond
    (instance? PdfException e)
    (let [^PdfException e e]
      (ex-info (.getMessage e)
               (cond-> {:type (keyword "pdf" (kebab (.getSimpleName (class e))))
                        :code (.code e)}
                 (.field e) (assoc :key (keyword (kebab (.field e)))))
               e))
    (instance? IllegalStateException e)
    (ex-info (.getMessage e) {:type :pdf/illegal-state} e)))

(defmacro ^:private translating
  "Run body, rethrowing Bolivar's Java failures as ex-info."
  [& body]
  `(try
     ~@body
     (catch clojure.lang.ExceptionInfo e# (throw e#))
     (catch RuntimeException e#
       (throw (or (translate e#) e#)))))

;; ---- options -------------------------------------------------------------------------------

(def ^:private option-keys #{:password :pages :max-pages :caching :layout :bidi})
(def ^:private layout-keys
  #{:line-overlap :char-margin :line-margin :word-margin :boxes-flow :detect-vertical :all-texts})
(def ^:private table-keys
  #{:vertical-strategy :horizontal-strategy
    :snap-tolerance :snap-x-tolerance :snap-y-tolerance
    :join-tolerance :join-x-tolerance :join-y-tolerance
    :intersection-tolerance :intersection-x-tolerance :intersection-y-tolerance
    :explicit-vertical-lines :explicit-horizontal-lines
    :crop :first-page-crop :max-pages})

(defn- check-map! [m what]
  (when-not (or (nil? m) (map? m))
    (invalid! nil (str what " must be a map") m)))

(defn- check-keys! [m allowed what]
  (when-let [unknown (seq (remove allowed (keys m)))]
    (invalid! (first unknown) (str "Unknown " what " " (pr-str (vec unknown))) m)))

(defn- int-value ^long [key v]
  (when-not (and (integer? v) (<= Integer/MIN_VALUE v Integer/MAX_VALUE))
    (invalid! key (str key " must be an integer") v))
  (long v))

(defn- number-value ^double [key v]
  (when-not (number? v) (invalid! key (str key " must be a number") v))
  (double v))

(defn- bool-value [key v]
  (when-not (boolean? v) (invalid! key (str key " must be true or false") v))
  v)

(defn- layout-options ^LayoutOptions [m]
  (check-map! m ":layout")
  (check-keys! m layout-keys "layout option")
  (let [b (LayoutOptions/builder)]
    (when (contains? m :line-overlap) (.lineOverlap b (number-value :line-overlap (:line-overlap m))))
    (when (contains? m :char-margin) (.charMargin b (number-value :char-margin (:char-margin m))))
    (when (contains? m :line-margin) (.lineMargin b (number-value :line-margin (:line-margin m))))
    (when (contains? m :word-margin) (.wordMargin b (number-value :word-margin (:word-margin m))))
    (when (contains? m :boxes-flow)
      (if-some [flow (:boxes-flow m)]
        (.boxesFlow b (number-value :boxes-flow flow))
        (.disableBoxesFlow b)))
    (when (contains? m :detect-vertical)
      (.detectVertical b (boolean (bool-value :detect-vertical (:detect-vertical m)))))
    (when (contains? m :all-texts)
      (.allTexts b (boolean (bool-value :all-texts (:all-texts m)))))
    (.build b)))

(defn- options ^Options [m]
  (check-map! m "options")
  (if (empty? m)
    Options/DEFAULT
    (do
      (check-keys! m option-keys "option")
      (let [b (Options/builder)]
        (when-some [password (:password m)]
          (when-not (string? password) (invalid! :password ":password must be a string" password))
          (.password b ^String password))
        (when (contains? m :pages)
          (let [pages (:pages m)]
            (when-not (sequential? pages)
              (invalid! :pages ":pages must be a sequence of 1-based page numbers" pages))
            (.pages b ^java.util.List (mapv #(Integer/valueOf (int (int-value :pages %))) pages))))
        (when (contains? m :max-pages) (.maxPages b (int (int-value :max-pages (:max-pages m)))))
        (when (contains? m :caching) (.caching b (boolean (bool-value :caching (:caching m)))))
        (when (contains? m :bidi) (.bidi b (boolean (bool-value :bidi (:bidi m)))))
        (when (contains? m :layout) (.layout b (layout-options (:layout m))))
        (.build b)))))

(defn- strategy ^TableStrategy [key v]
  (let [value (cond (keyword? v) (str/replace (name v) "-" "_")
                    (string? v) v
                    :else (invalid! key (str key " must be a keyword or string") v))]
    (try (TableStrategy/parse value)
         (catch IllegalArgumentException e (invalid! key (.getMessage e) v)))))

(defn- crop ^BoundingBox [key v]
  (when-not (and (sequential? v) (= 4 (count v)) (every? number? v))
    (invalid! key (str key " must be [x0 y0 x1 y1]") v))
  (let [[x0 y0 x1 y1] v]
    (BoundingBox. (double x0) (double y0) (double x1) (double y1))))

(defn- lines ^java.util.List [key v]
  (when-not (and (sequential? v) (every? number? v))
    (invalid! key (str key " must be a sequence of numbers") v))
  (mapv double v))

(defn- table-options ^TableOptions [m]
  (check-map! m "table options")
  (if (empty? m)
    TableOptions/DEFAULT
    (do
      (check-keys! m table-keys "table option")
      (let [b (TableOptions/builder)
            num (fn [k] (number-value k (get m k)))]
        (when-some [v (:vertical-strategy m)] (.verticalStrategy b (strategy :vertical-strategy v)))
        (when-some [v (:horizontal-strategy m)] (.horizontalStrategy b (strategy :horizontal-strategy v)))
        (when (some? (:snap-tolerance m)) (.snapTolerance b (num :snap-tolerance)))
        (when (some? (:snap-x-tolerance m)) (.snapXTolerance b (num :snap-x-tolerance)))
        (when (some? (:snap-y-tolerance m)) (.snapYTolerance b (num :snap-y-tolerance)))
        (when (some? (:join-tolerance m)) (.joinTolerance b (num :join-tolerance)))
        (when (some? (:join-x-tolerance m)) (.joinXTolerance b (num :join-x-tolerance)))
        (when (some? (:join-y-tolerance m)) (.joinYTolerance b (num :join-y-tolerance)))
        (when (some? (:intersection-tolerance m)) (.intersectionTolerance b (num :intersection-tolerance)))
        (when (some? (:intersection-x-tolerance m)) (.intersectionXTolerance b (num :intersection-x-tolerance)))
        (when (some? (:intersection-y-tolerance m)) (.intersectionYTolerance b (num :intersection-y-tolerance)))
        (when-some [v (:explicit-vertical-lines m)]
          (.explicitVerticalLines b (lines :explicit-vertical-lines v)))
        (when-some [v (:explicit-horizontal-lines m)]
          (.explicitHorizontalLines b (lines :explicit-horizontal-lines v)))
        (when-some [v (:crop m)] (.crop b (crop :crop v)))
        (when-some [v (:first-page-crop m)] (.firstPageCrop b (crop :first-page-crop v)))
        (when (some? (:max-pages m)) (.maxPages b (int (int-value :max-pages (:max-pages m)))))
        (.build b)))))

;; ---- views ---------------------------------------------------------------------------------

(def ^:private missing (Object.))

(declare realize)

(defn- map-equiv [m o]
  (and (instance? java.util.Map o)
       (= (count m) (.size ^java.util.Map o))
       (every? (fn [^MapEntry e]
                 (let [^java.util.Map o o]
                   (and (.containsKey o (.key e))
                        (= (.val e) (.get o (.key e))))))
               (seq m))))

(deftype RowView [kind ^IFn lookup keys]
  ;; `lookup` returns the value of a key, or `missing`. `keys` is shared by every row of a kind.
  ILookup
  (valAt [_ k] (let [v (.invoke lookup k)] (if (identical? v missing) nil v)))
  (valAt [_ k nf] (let [v (.invoke lookup k)] (if (identical? v missing) nf v)))

  IKeywordLookup
  (getLookupThunk [this k]
    ;; Every RowView answers every key (nil when absent), so the class alone qualifies.
    (let [c (class this)]
      (reify ILookupThunk
        (get [thunk target]
          (if (instance? c target)
            (.valAt ^ILookup target k)
            thunk)))))

  IFn
  (invoke [this k] (.valAt this k))
  (invoke [this k nf] (.valAt this k nf))
  (applyTo [this args] (clojure.lang.AFn/applyToHelper this args))

  Counted
  (count [_] (count keys))

  Associative
  (containsKey [_ k] (not (identical? (.invoke lookup k) missing)))
  (entryAt [_ k] (let [v (.invoke lookup k)] (when-not (identical? v missing) (MapEntry/create k v))))
  (assoc [this k v] (assoc (realize this) k v))

  MapEquivalence

  IPersistentMap
  (assocEx [this k v] (.assocEx ^IPersistentMap (realize this) k v))
  (without [this k] (dissoc (realize this) k))

  IPersistentCollection
  (cons [this o] (conj (realize this) o))
  (empty [_] {})
  (equiv [this o] (map-equiv this o))

  Seqable
  (seq [_] (seq (map (fn [k] (MapEntry/create k (.invoke lookup k))) keys)))

  Iterable
  (iterator [this] (let [^Iterable s (or (seq this) ())] (.iterator s)))

  IHashEq
  (hasheq [this] (Murmur3/hashUnordered this))

  java.util.Map
  (size [_] (count keys))
  (isEmpty [_] (empty? keys))
  (containsValue [this v] (boolean (some #(= v (val %)) (seq this))))
  (get [this k] (.valAt this k))
  (keySet [_] (java.util.LinkedHashSet. ^java.util.Collection keys))
  (values [this] (mapv val (seq this)))
  (entrySet [this] (java.util.LinkedHashSet. ^java.util.Collection (vec (seq this))))
  (put [_ _ _] (throw (UnsupportedOperationException.)))
  (remove [_ _] (throw (UnsupportedOperationException.)))
  (putAll [_ _] (throw (UnsupportedOperationException.)))
  (clear [_] (throw (UnsupportedOperationException.)))

  Object
  (equals [this o] (clojure.lang.APersistentMap/mapEquals this o))
  (hashCode [this] (clojure.lang.APersistentMap/mapHash this))
  (toString [this] (pr-str this)))

(declare reduce-from)

(deftype IndexedView [^long n ^IFn item]
  ;; An immutable, lazily built sequence of `n` items; `item` maps an index to its item.
  Counted
  (count [_] n)

  Indexed
  (nth [_ i] (if (and (<= 0 i) (< i n)) (.invoke item i) (throw (IndexOutOfBoundsException. (str i)))))
  (nth [_ i nf] (if (and (<= 0 i) (< i n)) (.invoke item i) nf))

  Sequential

  Seqable
  (seq [_] (when (pos? n) (map #(.invoke item %) (range n))))

  IReduceInit
  (reduce [_ f init]
    (loop [i 0 acc init]
      (cond
        (instance? Reduced acc) @acc
        (== i n) acc
        :else (recur (inc i) (f acc (.invoke item i))))))

  IReduce
  (reduce [this f]
    (if (zero? n)
      (f)
      (reduce-from this f (.invoke item 0) 1)))

  IPersistentCollection
  (cons [this o] (conj (vec this) o))
  (empty [_] [])
  (equiv [this o]
    (and (or (sequential? o) (instance? java.util.List o))
         (= n (count o))
         (= (seq this) (seq o))))

  Iterable
  (iterator [this] (let [^Iterable s (or (seq this) ())] (.iterator s)))

  IHashEq
  (hasheq [this] (Murmur3/hashOrdered this))

  Object
  (equals [this o] (.equiv this o))
  (hashCode [this] (clojure.lang.Util/hash (vec this)))
  (toString [this] (pr-str this)))

(defmethod print-method RowView [v ^java.io.Writer w]
  (print-method (into {} (seq v)) w))

(defmethod print-method IndexedView [v ^java.io.Writer w]
  (print-method (vec v) w))

(defn- reduce-from [^IndexedView v f init ^long start]
  (loop [i start acc init]
    (cond
      (instance? Reduced acc) @acc
      (== i (.-n v)) acc
      :else (recur (inc i) (f acc (.invoke ^IFn (.-item v) i))))))

(defn- indexed ^IndexedView [^long start ^long end item]
  (IndexedView. (- end start) (fn [^long i] (item (+ start i)))))

(defn- bbox-map [^BoundingBox b]
  {:x0 (.x0 b) :y0 (.y0 b) :x1 (.x1 b) :y1 (.y1 b)})

(def ^:private char-keys [:text :bbox :x0 :y0 :x1 :y1 :font-name :size :upright])
(def ^:private line-keys [:bbox :orientation :text :chars])
(def ^:private box-keys [:bbox :writing-mode :text :lines])
(def ^:private page-keys [:page-number :bbox :rotate :text :text-boxes :lines :chars])

(defn- layout-char [^LayoutPage p ^long i]
  (RowView. ::char
            (fn [k]
              (case k
                :text (.charText p i)
                :x0 (.charX0 p i)
                :y0 (.charY0 p i)
                :x1 (.charX1 p i)
                :y1 (.charY1 p i)
                :bbox {:x0 (.charX0 p i) :y0 (.charY0 p i) :x1 (.charX1 p i) :y1 (.charY1 p i)}
                :font-name (.charFontName p i)
                :size (.charSize p i)
                :upright (.charUpright p i)
                missing))
            char-keys))

(defn- layout-line [^LayoutPage p ^long l]
  (RowView. ::line
            (fn [k]
              (case k
                :bbox (bbox-map (.lineBbox p l))
                :orientation (.lineOrientation p l)
                :text (.lineText p l)
                :chars (indexed (.lineCharStart p l) (.lineCharEnd p l) #(layout-char p %))
                missing))
            line-keys))

(defn- layout-box [^LayoutPage p ^long b]
  (RowView. ::box
            (fn [k]
              (case k
                :bbox (bbox-map (.textBoxBbox p b))
                :writing-mode (.textBoxWritingMode p b)
                :text (.textBoxText p b)
                :lines (indexed (.textBoxLineStart p b) (.textBoxLineEnd p b) #(layout-line p %))
                missing))
            box-keys))

(defn- layout-page [^LayoutPage p]
  (RowView. ::page
            (fn [k]
              (case k
                :page-number (.pageNumber p)
                :bbox (bbox-map (.bbox p))
                :rotate (.rotate p)
                :text (.text p)
                :text-boxes (indexed 0 (.textBoxCount p) #(layout-box p %))
                :lines (indexed 0 (.lineCount p) #(layout-line p %))
                :chars (indexed 0 (.charCount p) #(layout-char p %))
                ::page p
                missing))
            page-keys))

(def ^:private raw-char-keys
  [:text :bbox :x0 :y0 :x1 :y1 :font-name :size :upright :advance :matrix :marked-content-id
   :tag :non-stroking-color-space :stroking-color-space :non-stroking-color :stroking-color])
(def ^:private raw-line-keys [:bbox :orientation :raw-text :text :chars])
(def ^:private raw-page-keys
  [:page-index :page-number :object-id :label :rotation :user-unit :boxes :layout-bbox :text
   :text-boxes :lines :chars :tables])

(defn- some-vec [xs] (some-> xs vec))

(defn- raw-char [^RawPage p ^long i]
  (RowView. ::raw-char
            (fn [k]
              (case k
                :text (.charText p i)
                :x0 (.charX0 p i)
                :y0 (.charY0 p i)
                :x1 (.charX1 p i)
                :y1 (.charY1 p i)
                :bbox {:x0 (.charX0 p i) :y0 (.charY0 p i) :x1 (.charX1 p i) :y1 (.charY1 p i)}
                :font-name (.charFontName p i)
                :size (.charSize p i)
                :upright (.charUpright p i)
                :advance (.charAdvance p i)
                :matrix (mapv #(.charMatrix p i (int %)) (range 6))
                :marked-content-id (.charMarkedContentId p i)
                :tag (.charTag p i)
                :non-stroking-color-space (.charNonStrokingColorSpace p i)
                :stroking-color-space (.charStrokingColorSpace p i)
                :non-stroking-color (some-vec (.charNonStrokingColor p i))
                :stroking-color (some-vec (.charStrokingColor p i))
                missing))
            raw-char-keys))

(defn- raw-line [^RawPage p ^long l]
  (RowView. ::raw-line
            (fn [k]
              (case k
                :bbox (bbox-map (.lineBbox p l))
                :orientation (.lineOrientation p l)
                :raw-text (.lineRawText p l)
                :text (.lineText p l)
                :chars (indexed (.lineCharStart p l) (.lineCharEnd p l) #(raw-char p %))
                missing))
            raw-line-keys))

(defn- raw-box [^RawPage p ^long b]
  (RowView. ::raw-box
            (fn [k]
              (case k
                :bbox (bbox-map (.textBoxBbox p b))
                :writing-mode (.textBoxWritingMode p b)
                :text (.textBoxText p b)
                :lines (indexed (.textBoxLineStart p b) (.textBoxLineEnd p b) #(raw-line p %))
                missing))
            box-keys))

(defn- raw-table-bbox [^RawTableBoundingBox b]
  {:x0 (.x0 b) :top (.top b) :x1 (.x1 b) :bottom (.bottom b)})

(defn- raw-table [^RawTable t]
  {:bbox (raw-table-bbox (.bbox t))
   :row-count (.rowCount t)
   :column-count (.columnCount t)
   :cells (mapv (fn [^sa.ingenious.pdf.RawTableCell c]
                  {:row-index (.rowIndex c) :column-index (.columnIndex c)
                   :row-span (.rowSpan c) :column-span (.columnSpan c)
                   :bbox (raw-table-bbox (.bbox c)) :text (.text c)})
                (.cells t))})

(defn- raw-page-view [^RawPage p]
  (RowView. ::raw-page
            (fn [k]
              (case k
                :page-index (.pageIndex p)
                :page-number (.pageNumber p)
                :object-id (.objectId p)
                :label (.label p)
                :rotation (.rotation p)
                :user-unit (.userUnit p)
                :boxes (let [b (.boxes p)]
                         {:media (some-vec (.media b)) :crop (some-vec (.crop b))
                          :bleed (some-vec (.bleed b)) :trim (some-vec (.trim b))
                          :art (some-vec (.art b))})
                :layout-bbox (bbox-map (.layoutBbox p))
                :text (.text p)
                :text-boxes (indexed 0 (.textBoxCount p) #(raw-box p %))
                :lines (indexed 0 (.lineCount p) #(raw-line p %))
                :chars (indexed 0 (.charCount p) #(raw-char p %))
                :tables (mapv raw-table (.tables p))
                ::page p
                missing))
            raw-page-keys))

(defn realize
  "Copy a view, and every view inside it, into plain maps and vectors. The result keeps nothing
  native or columnar alive. Other values pass through unchanged."
  [x]
  (cond
    (instance? RowView x) (into {} (map (fn [[k v]] [k (realize v)])) (seq x))
    (instance? IndexedView x) (into [] (map realize) x)
    (map? x) (into {} (map (fn [[k v]] [k (realize v)])) x)
    (vector? x) (mapv realize x)
    :else x))

(defn view?
  "Whether x is a view over page columns (see `realize`)."
  [x]
  (or (instance? RowView x) (instance? IndexedView x)))

(defn columns
  "A layout or raw page's character columns as fresh arrays: doubles for :x0 :y0 :x1 :y1 :size,
  strings for :font-name and :text."
  [page]
  (let [p (get page ::page)]
    (cond
      (instance? LayoutPage p)
      (let [^LayoutPage p p]
        {:x0 (.charX0s p) :y0 (.charY0s p) :x1 (.charX1s p) :y1 (.charY1s p)
         :size (.charSizes p) :font-name (.charFontNames p)
         :text (into-array String (map #(.charText p (int %)) (range (.charCount p))))})
      (instance? RawPage p)
      (let [^RawPage p p]
        {:x0 (.charX0s p) :y0 (.charY0s p) :x1 (.charX1s p) :y1 (.charY1s p)
         :size (.charSizes p) :advance (.charAdvances p) :font-name (.charFontNames p)
         :text (into-array String (map #(.charText p (int %)) (range (.charCount p))))})
      :else (fail! "columns takes a page from `pages` or `raw-pages`"
                   {:type :pdf/invalid-argument :value page}))))

;; ---- small records as plain maps -----------------------------------------------------------

(defn- page-text-map [^PageText p]
  {:page-number (.pageNumber p)
   :bbox (bbox-map (.bbox p))
   :text-boxes (mapv (fn [^sa.ingenious.pdf.PageTextBox b]
                       {:bbox (bbox-map (.bbox b)) :text (.text b) :text-without-bidi (.textWithoutBidi b)
                        :lines (mapv (fn [^sa.ingenious.pdf.PageTextLine l]
                                       {:bbox (bbox-map (.bbox l)) :text (.text l)
                                        :text-without-bidi (.textWithoutBidi l)})
                                     (.lines b))})
                     (.textBoxes p))
   :fonts (set (.fonts p))
   :digit-styles (into #{} (map (fn [^sa.ingenious.pdf.DigitStyle s] [(.fontName s) (long (.size s))]))
                       (.digitStyles p))})

(defn- summary-map [^PageSummary s]
  {:page-number (.pageNumber s) :text (.text s) :bbox (bbox-map (.bbox s)) :rotate (.rotate s)})

(defn- table-map [^Table t]
  {:page-number (.pageNumber t)
   :bbox (bbox-map (.bbox t))
   :row-count (.rowCount t)
   :column-count (.columnCount t)
   :cells (mapv (fn [^sa.ingenious.pdf.TableCell c]
                  {:row-index (.rowIndex c) :column-index (.columnIndex c)
                   :row-span (.rowSpan c) :column-span (.columnSpan c)
                   :bbox (bbox-map (.bbox c)) :text (.text c)})
                (.cells t))})

(defn- table-rows-map [^PageTableRows r]
  {:page-number (.pageNumber r)
   :cells (vec (.cells r))
   :row-offsets (vec (.rowOffsets r))
   :table-offsets (vec (.tableOffsets r))})

(defn- metadata-map [^Metadata m]
  (let [v (.version m) p (.permissions m)]
    {:document-info (into {} (map (fn [^MetadataEntry e] [(.key e) (.value e)])) (.documentInfo m))
     :title (.title m) :author (.author m) :subject (.subject m) :keywords (.keywords m)
     :creator (.creator m) :producer (.producer m)
     :creation-date-raw (.creationDateRaw m) :creation-date-iso (.creationDateIso m)
     :modification-date-raw (.modificationDateRaw m) :modification-date-iso (.modificationDateIso m)
     :version {:header (.header v) :catalog (.catalog v) :effective (.effective v)}
     :file-size-bytes (.fileSizeBytes m) :page-count (.pageCount m) :encrypted (.encrypted m)
     :permissions {:printable (.printable p) :modifiable (.modifiable p) :extractable (.extractable p)}
     :linearized (.linearized m) :tagged (.tagged m) :user-properties (.userProperties m)
     :suspects (.suspects m) :form (.form m) :has-javascript (.hasJavascript m)
     :has-metadata-stream (.hasMetadataStream m) :xmp-metadata (.xmpMetadata m)}))

;; ---- documents -----------------------------------------------------------------------------

(defn open
  "Open a PDF and return a document; close it with `with-open`. `source` is a path string,
  java.nio.file.Path, java.io.File, java.io.InputStream (read to its end and closed) or byte
  array. A file is memory-mapped: keep it unchanged while the document is open.

  `opts` keys: :password, :pages (1-based page numbers), :max-pages, :caching, :bidi, and
  :layout with :line-overlap :char-margin :line-margin :word-margin :boxes-flow (nil turns the
  ordering off) :detect-vertical :all-texts."
  (^Document [source] (open source nil))
  (^Document [source opts]
   (let [o (options opts)]
     (translating
      (cond
        (string? source) (Document/open ^String source o)
        (instance? Path source) (Document/open ^Path source o)
        (instance? File source) (Document/open ^File source o)
        (instance? InputStream source) (Document/open ^InputStream source o)
        (bytes? source) (Document/open ^bytes source o)
        :else (fail! "Unsupported source; expected a path string, Path, File, InputStream or byte array"
                     {:type :pdf/invalid-argument :key :source
                      :value (some-> source class .getName)}))))))

(defn text
  "The whole document's text, pages separated by form feeds."
  [^Document doc]
  (translating (.text doc)))

(defn extract-text
  "Open `source`, read its text, and close it."
  ([source] (extract-text source nil))
  ([source opts]
   (with-open [doc (open source opts)]
     (text doc))))

(defn metadata
  "Canonical and raw document metadata, as a map."
  [^Document doc]
  (translating (metadata-map (.metadata doc))))

(defn page-count
  "The number of pages the document's options select."
  [^Document doc]
  (translating (.pageCount doc)))

(defn declared-page-count
  "The number of pages the file declares."
  [^Document doc]
  (translating (.declaredPageCount doc)))

(defn raw-page
  "One 1-based page with nothing dropped, as a view."
  [^Document doc page-number]
  (translating (raw-page-view (.rawPage doc (int (int-value :page-number page-number))))))

(defn version
  "The Bolivar version."
  []
  (translating (Document/version)))

(defn- source
  "A reducible over a cursor that `open-cursor` opens. The cursor opens and closes inside each
  reduction, so the source may be reduced more than once."
  [open-cursor item]
  ;; Only Bolivar's own calls are translated; whatever `f` throws passes through untouched.
  (letfn [(run [f init]
            (let [^Cursor cursor (translating (open-cursor))]
              (try
                (loop [acc init]
                  (cond
                    (reduced? acc) @acc
                    (translating (.hasNext cursor)) (recur (f acc (item (translating (.next cursor)))))
                    :else acc))
                (finally (.close cursor)))))]
    (reify
      IReduceInit
      (reduce [_ f init] (run f init))
      IReduce
      (reduce [_ f]
        (let [none (Object.)
              result (run (fn [acc x] (if (identical? acc none) x (f acc x))) none)]
          (if (identical? result none) (f) result))))))

(defn pages
  "The laid-out pages, as views with :page-number :bbox :rotate :text :text-boxes, and every
  :lines and :chars of the page. Boxes hold :lines, lines hold :chars; characters have :text
  :bbox :x0 :y0 :x1 :y1 :font-name :size :upright."
  [^Document doc]
  (source #(.pages doc) layout-page))

(defn raw-pages
  "Every page with nothing dropped, as views: PDF identity and :boxes, :text-boxes, :lines with
  :raw-text, :chars with their graphics state, and :tables in pdfplumber space."
  [^Document doc]
  (source #(.rawPages doc) raw-page-view))

(defn page-texts
  "Each page's boxes and lines with :text and :text-without-bidi, its :fonts, and the
  :digit-styles [font whole-size] of its digits. Plain maps; no per-character data."
  [^Document doc]
  (source #(.pageTexts doc) page-text-map))

(defn page-summaries
  "Each page's :page-number :text :bbox :rotate, as plain maps."
  [^Document doc]
  (source #(.pageSummaries doc) summary-map))

(defn tables
  "Every table of every page, as plain maps with :page-number :bbox :row-count :column-count
  and :cells. `opts` uses pdfplumber's vocabulary: :vertical-strategy and
  :horizontal-strategy (:lines :lines-strict :text :explicit), :snap-tolerance
  :join-tolerance :intersection-tolerance and their -x-/-y- forms,
  :explicit-vertical-lines :explicit-horizontal-lines, :crop and :first-page-crop as
  [x0 y0 x1 y1], and :max-pages."
  ([doc] (tables doc nil))
  ([^Document doc opts]
   (let [o (table-options opts)]
     (source #(.tables doc o) table-map))))

(defn table-rows
  "The raw table rows of each page, exactly as the pdfplumber-compatible pipeline emits them:
  :cells (nil for an empty cell), :row-offsets into cells and :table-offsets into rows. Takes
  the same options as `tables`."
  ([doc] (table-rows doc nil))
  ([^Document doc opts]
   (let [o (table-options opts)]
     (source #(.tableRows doc o) table-rows-map))))
