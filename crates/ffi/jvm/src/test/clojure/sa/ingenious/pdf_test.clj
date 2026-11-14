(ns sa.ingenious.pdf-test
  (:require [clojure.datafy :refer [datafy]]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [sa.ingenious.corpus :as corpus]
            [sa.ingenious.pdf :as pdf])
  (:import [java.lang.ref WeakReference]
           [sa.ingenious.pdf.internal Native]))

(def simple (str corpus/repo "/crates/core/tests/fixtures/simple1.pdf"))
(def nics (str corpus/repo "/crates/core/tests/fixtures/pdfplumber/nics-background-checks-2015-11.pdf"))
(def big (str corpus/repo "/crates/core/tests/fixtures/nonfree/i1040nr.pdf"))

(defn- live [] (Native/liveHandles))

(defn- await-handles [expected]
  (loop [tries 0]
    (when (and (not= expected (live)) (< tries 500))
      (System/gc)
      (Thread/sleep 10)
      (recur (inc tries))))
  (is (= expected (live)) "native handles return to their count"))

;; ---- parity with the corpus, through the Clojure API -------------------------------------

(def ^:private char-extra [:x0 :y0 :x1 :y1])

(defn- old-chars [chars] (mapv #(apply dissoc % char-extra) chars))

(defn- old-layout-page [page]
  (-> (pdf/realize page)
      (dissoc :lines :chars)
      (update :text-boxes
              (fn [boxes]
                (mapv (fn [b] (update b :lines (fn [ls] (mapv #(update % :chars old-chars) ls)))) boxes)))))

(defn- old-raw-page [page]
  (-> (pdf/realize page)
      (dissoc :lines :chars)
      (update :text-boxes
              (fn [boxes]
                (mapv (fn [b]
                        (update b :lines
                                (fn [ls]
                                  (mapv (fn [l] (-> l (assoc :characters (old-chars (:chars l)))
                                                    (dissoc :chars)))
                                        ls))))
                      boxes)))))

(defn- old-error->type
  "The ::pdf/error the new API reports where the old one failed."
  [{:keys [error cause]}]
  (cond
    (= error :pdf/invalid-options) :invalid-argument
    cause (keyword (-> cause (str/replace #"([a-z])([A-Z])" "$1-$2") str/lower-case))
    :else (keyword (name error))))

(defn- normalise [v]
  (if (and (map? v) (contains? v :error) (contains? v :cause))
    {:error (old-error->type v)}
    v))

(defn- attempt [f]
  (try (f) (catch clojure.lang.ExceptionInfo e {:error (::pdf/error (ex-data e))})))

(defn- table-opts [m]
  ;; The old API took strategies as strings; the new one takes keywords or strings.
  m)

(defn- replay [{:keys [opts tables] :as case} expected]
  (try
    (with-open [d (pdf/open (corpus/path case) opts)]
      (let [calls {:text #(pdf/text d)
                   :layout-pages #(into [] (map old-layout-page) (pdf/pages d))
                   :raw-document #(let [pages (into [] (map old-raw-page) (pdf/raw-pages d))]
                                    {:declared-page-count (pdf/declared-page-count d)
                                     :page-count (count pages)
                                     :pages pages})
                   :raw-page-1 #(old-raw-page (pdf/raw-page d 1))
                   :page-texts #(into [] (pdf/page-texts d))
                   :page-summaries #(into [] (pdf/page-summaries d))
                   :metadata #(pdf/metadata d)}]
        (merge
         (into {} (for [k (keys (dissoc expected :tables-by-opts :open))]
                    [k (attempt (calls k))]))
         {:tables-by-opts
          (mapv (fn [t]
                  {:tables (attempt #(into [] (pdf/tables d (table-opts t))))
                   :table-rows (attempt #(into [] (pdf/table-rows d (table-opts t))))})
                tables)})))
    (catch clojure.lang.ExceptionInfo e {:open {:error (::pdf/error (ex-data e))}})))

(defn- normalise-results [results]
  (cond-> (into {} (map (fn [[k v]] [k (normalise v)])) results)
    (:tables-by-opts results)
    (update :tables-by-opts (fn [ts] (mapv #(into {} (map (fn [[k v]] [k (normalise v)])) %) ts)))))

(deftest clojure-api-matches-the-corpus
  (doseq [file (corpus/files)]
    (let [{:keys [case results]} (corpus/read-case file)
          expected (normalise-results results)
          actual (replay case results)]
      (doseq [k (into (sorted-set) (concat (keys expected) (keys actual)))]
        (is (= (get expected k) (get actual k)) (str (:id case) " " k))))))

;; ---- behaviour ---------------------------------------------------------------------------

(deftest sources-free-their-cursor-on-reduced-and-on-errors
  (let [before (live)]
    (with-open [d (pdf/open big)]
      (is (= 2 (reduce (fn [n _] (if (= n 1) (reduced (inc n)) (inc n))) 0 (pdf/pages d))))
      (is (= :boom (try (run! (fn [_] (throw (ex-info "boom" {:boom true}))) (pdf/pages d))
                        (catch clojure.lang.ExceptionInfo e (when (:boom (ex-data e)) :boom)))))
      (is (= 3 (count (into [] (take 3) (pdf/page-summaries d)))))
      (testing "sources reduce more than once"
        (let [source (pdf/page-summaries d)]
          (is (= (into [] source) (into [] source))))))
    (await-handles before)))

(deftest reduce-without-init
  (with-open [d (pdf/open simple)]
    (is (= 0 (reduce + (pdf/tables d))) "an empty source returns (f)")
    (is (= 1 (:page-number (reduce (fn [a _] a) (pdf/page-summaries d)))))))

(deftest views-are-maps
  (with-open [d (pdf/open nics)]
    (let [page (first (into [] (pdf/pages d)))
          ch (nth (:chars page) 0)
          plain (pdf/realize ch)]
      (is (pdf/view? ch))
      (is (not (pdf/view? plain)))
      (is (= ch plain))
      (is (= plain ch))
      (is (= (hash ch) (hash plain)))
      (is (= 1 (count (hash-set ch plain))))
      (is (= 1 (get {plain 1} ch)))
      (is (= (:chars page) (pdf/realize (:chars page))))
      (is (= #{:text :bbox :x0 :y0 :x1 :y1 :font-name :size :upright} (set (keys ch))))
      (is (= (count ch) (reduce-kv (fn [n _ _] (inc n)) 0 ch)))
      (is (= :x0 (reduce-kv (fn [_ k _] (reduced k)) nil (select-keys ch [:x0]))))
      (is (= {:a 1} (meta (with-meta ch {:a 1}))))
      (is (= plain (with-meta ch {:a 1})) "metadata does not change equality")
      (is (= plain (datafy ch)))
      (is (= (into {} ch) plain))
      (is (= (:x0 ch) (get-in ch [:bbox :x0])))
      (is (nil? (:nope ch)))
      (is (= :d (get ch :nope :d)))
      (is (not (pdf/view? (assoc ch :extra 1))) "assoc realises")
      (is (= 4319 (count (:chars page))))
      (is (= (count (:chars page))
             (reduce + (map (comp count :chars) (:lines page)))))
      (is (= (pr-str plain) (pr-str ch)) "views print as maps")
      (let [cols (pdf/columns page)]
        (is (= (:x0 ch) (aget ^doubles (:x0 cols) 0)))
        (is (= (:text ch) (aget ^"[Ljava.lang.String;" (:text cols) 0)))))))

(deftest view-collections-read-as-vectors
  ;; :chars, :lines and :text-boxes answer what their realized vectors answer.
  (with-open [d (pdf/open nics)]
    (let [page (first (into [] (pdf/pages d)))
          chars (:chars page)
          plain (pdf/realize chars)]
      (is (vector? chars))
      (is (= (get-in plain [0 :x0]) (get-in page [:chars 0 :x0])))
      (is (contains? chars 0))
      (is (not (contains? chars (count chars))))
      (is (nil? (get chars -1)))
      (is (= (subvec plain 1 3) (subvec chars 1 3)))
      (is (= (peek plain) (peek chars)))
      (is (= (peek plain) (first (rseq chars))))
      (is (= (pop plain) (pop chars)))
      (is (= (assoc plain 0 :z) (assoc chars 0 :z)))
      (is (= (hash plain) (hash chars))))))

(defn- realise-first-char
  "A weak reference to a page and a realised copy of its first character."
  []
  (with-open [d (pdf/open nics)]
    (let [page (first (into [] (pdf/pages d)))]
      [(WeakReference. (pdf/java-page page)) (pdf/realize (nth (:chars page) 0))])))

(deftest realize-detaches-from-the-page
  (let [[^WeakReference ref kept] (realise-first-char)]
    ;; `some?` keeps the referent out of a loop local, which Clojure would not clear.
    (loop [tries 0]
      (when (and (some? (.get ref)) (< tries 200))
        (System/gc)
        (Thread/sleep 10)
        (recur (inc tries))))
    (is (nil? (.get ref)) "a realised value does not pin its page")
    (is (= "S" (:text kept)))))

(deftest errors-are-anomalies-with-error-code-and-key
  (let [data (fn [f] (try (f) nil (catch clojure.lang.ExceptionInfo e (ex-data e))))
        error (comp ::pdf/error data)
        key (comp ::pdf/key data)]
    (let [d (data #(pdf/open "nope.pdf"))]
      (is (= :io-not-found (::pdf/error d)))
      (is (= :cognitect.anomalies/not-found (:cognitect.anomalies/category d)))
      (is (= 3 (::pdf/code d)))
      (is (string? (:cognitect.anomalies/message d))))
    (is (= :max-pages (key #(pdf/open simple {:max-pages 0}))))
    (is (= :pages (key #(pdf/open simple {:pages [0]}))))
    (is (= :bogus (key #(pdf/open simple {:bogus 1}))))
    (is (= :caching (key #(pdf/open simple {:caching "yes"}))))
    (is (= :line-margin (key #(pdf/open simple {:layout {:line-margin "wide"}}))))
    (is (= :vertical-strategy (key #(pdf/tables nil {:vertical-strategy :sideways}))))
    (is (= :encryption-error
           (error #(pdf/open (str corpus/repo "/crates/core/tests/fixtures/encryption/rc4-40.pdf")
                             {:password "bar"}))))
    (with-open [d (pdf/open simple)]
      (is (= :snap-tolerance (key #(into [] (pdf/tables d {:snap-tolerance -1})))))
      (is (= :page-number (key #(pdf/raw-page d 0)))))
    (let [d (pdf/open simple)]
      (.close d)
      (is (= :illegal-state (error #(pdf/text d))))
      (is (= :cognitect.anomalies/conflict
             (:cognitect.anomalies/category (data #(pdf/text d))))))))

(def ^:private labels (str corpus/repo "/crates/core/tests/fixtures/contrib/pagelabels.pdf"))

(defn- page-numbers [source] (into [] (map :page-number) source))

(deftest every-source-takes-cursor-options
  (with-open [d (pdf/open labels)]
    (is (= [2 4] (page-numbers (pdf/pages d {:pages [2 4]}))))
    (is (= [2 4] (page-numbers (pdf/raw-pages d {:pages [4 2 2 9]}))))
    (is (= [1 2] (page-numbers (pdf/page-texts d {:max-pages 2}))))
    (is (= [4 5] (page-numbers (pdf/page-summaries d {:skip 3}))))
    (is (= [1 2] (page-numbers (pdf/table-rows d {:max-pages 2 :snap-tolerance 3}))))
    (is (= [1 2 3 4 5] (page-numbers (pdf/page-summaries d {:read-ahead 1}))))
    (is (= [] (page-numbers (pdf/page-summaries d {:pages []}))) "[] keeps no page")
    (is (= [1 2 3 4 5] (page-numbers (pdf/page-summaries d {:pages nil}))) "nil leaves it unset")))

(deftest a-head-and-a-skip-read-each-selected-page-once
  (with-open [d (pdf/open labels {:pages [2 3 5]})]
    (let [head (into [] (pdf/page-texts d {:max-pages 2}))
          rest (into [] (pdf/page-texts d {:skip 2}))]
      (is (= [2 3] (mapv :page-number head)))
      (is (= [5] (mapv :page-number rest)))
      (is (= (into [] (pdf/page-texts d)) (into head rest))))
    (is (= [3] (page-numbers (pdf/page-summaries d {:pages [1 3 4]})))
        "a source reads only pages the document selects")))

(deftest bad-cursor-options-throw-when-the-source-is-made
  (with-open [d (pdf/open labels)]
    (let [key-of (fn [f] (try (f) nil (catch clojure.lang.ExceptionInfo e (::pdf/key (ex-data e)))))]
      (is (= :read-ahead (key-of #(pdf/page-summaries d {:read-ahead "fast"}))))
      (is (= :pages (key-of #(pdf/pages d {:pages [1 :two]}))))
      (is (= :bogus (key-of #(pdf/page-texts d {:bogus 1}))))
      (is (= :bogus (key-of #(pdf/tables d {:bogus 1}))))
      (is (= :read-ahead (key-of #(into [] (pdf/page-summaries d {:read-ahead 0})))))
      (is (= :skip (key-of #(into [] (pdf/page-summaries d {:skip -1})))))
      (is (= :max-pages (key-of #(into [] (pdf/table-rows d {:max-pages 0}))))))))

(deftest nil-option-values-leave-options-unset
  (is (= (pdf/extract-text simple) (pdf/extract-text simple {:password nil :max-pages nil})))
  (with-open [d (pdf/open simple)]
    (is (= (into [] (pdf/tables d)) (into [] (pdf/tables d {:snap-tolerance nil}))))))

(deftest realized-pages-share-their-characters
  (with-open [d (pdf/open nics)]
    (let [page (pdf/realize (first (into [] (pdf/pages d))))
          line (first (:lines page))
          box (first (:text-boxes page))]
      (is (not (pdf/view? page)))
      (is (identical? (first (:chars page)) (first (:chars line))))
      (is (identical? (first (:lines page)) (first (:lines box))))
      (is (= (:lines page) (mapcat :lines (:text-boxes page)))))))

(deftest keyword-lookup-sites-handle-views-and-maps
  ;; One call site sees views of two types and plain maps, so its cached thunk must fall back.
  (with-open [d (pdf/open nics)]
    (let [page (first (into [] (pdf/pages d)))
          values [(nth (:chars page) 0) (first (:lines page)) {:text "plain"} page
                  (nth (:chars page) 1)]
          texts (mapv (fn [v] (:text v)) values)]
      (is (= [(:text (pdf/realize (nth (:chars page) 0)))
              (:text (pdf/realize (first (:lines page))))
              "plain"
              (:text page)
              (:text (pdf/realize (nth (:chars page) 1)))]
             texts)))))

(deftest sources-say-what-they-are
  (with-open [d (pdf/open simple)]
    (is (re-find #"reducible, not a seq" (str (pdf/pages d))))))

(deftest boxes-flow-nil-turns-ordering-off
  (let [paper (str corpus/repo "/crates/core/tests/fixtures/nonfree/naacl06-shinyama.pdf")
        first-text (fn [opts] (with-open [d (pdf/open paper opts)]
                                (:text (first (into [] (pdf/pages d))))))]
    (is (not= (first-text nil) (first-text {:layout {:boxes-flow nil}})))
    (is (= (first-text nil) (first-text {:layout {:boxes-flow 0.5}})))))

(deftest sources-and-text
  (let [bytes (java.nio.file.Files/readAllBytes (java.nio.file.Path/of simple (make-array String 0)))]
    (is (= (pdf/extract-text simple) (pdf/extract-text bytes)))
    (is (= (pdf/extract-text simple) (pdf/extract-text (java.io.ByteArrayInputStream. bytes))))
    (is (= (pdf/extract-text simple) (pdf/extract-text (java.io.File. ^String simple))))
    (is (string? (pdf/version)))))

(deftest caller-errors-are-not-relabelled
  (with-open [d (pdf/open simple)]
    (let [thrown (try (run! (fn [_] (throw (IllegalStateException. "my bug"))) (pdf/pages d))
                      (catch Exception e e))]
      (is (instance? IllegalStateException thrown))
      (is (= "my bug" (ex-message thrown))))))
