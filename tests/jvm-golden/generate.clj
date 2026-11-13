(ns generate
  "Capture what the JVM binding returns today, so a rewrite can prove parity.

  Run from the repository root with the current binding on the classpath and
  its native library loadable:

    java --enable-native-access=ALL-UNNAMED -cp <classpath> clojure.main
      -i tests/jvm-golden/generate.clj -e '(generate/-main)'

  Each case writes <out-dir>/<id>.edn.gz: the case, and for every call
  either its realised result or the error it raised."
  (:require [clojure.edn :as edn]
            [clojure.java.io :as io]
            [sa.ingenious.pdf :as pdf])
  (:import [java.util.zip GZIPOutputStream]))

(def fixtures "crates/core/tests/fixtures/")

(defn- realise
  "Turn Java collections and arrays inside a result into Clojure values."
  [x]
  (cond
    (map? x)                  (into {} (map (fn [[k v]] [k (realise v)])) x)
    (set? x)                  (into #{} (map realise) x)
    (instance? java.util.List x) (mapv realise x)
    (sequential? x)           (mapv realise x)
    (some-> x class .isArray) (mapv realise x)
    :else                     x))

(defn- error-data [^Throwable e]
  {:error (or (:type (ex-data e)) :unknown)
   :cause (some-> (.getCause e) class .getSimpleName)})

(defn- attempt [f]
  (try (realise (f)) (catch Exception e (error-data e))))

(defn- drain [cursor-fn]
  (fn [] (with-open [c (cursor-fn)] (vec (seq c)))))

(def doc-calls
  {:text           pdf/text
   :layout-pages   pdf/layout-pages
   :raw-document   pdf/raw-document
   :raw-page-1     #(pdf/raw-page % 1)
   :page-texts     (fn [d] ((drain #(pdf/page-texts d))))
   :page-summaries (fn [d] ((drain #(pdf/page-summaries d))))
   :metadata       pdf/metadata})

(defn- table-calls [table-opts]
  {:tables     (fn [d] ((drain #(pdf/tables d table-opts))))
   :table-rows (fn [d] ((drain #(pdf/table-rows d table-opts))))})

(def cases
  "Each case: :id, :file (under fixtures) or :path, :opts (document options or nil),
  :calls (subset of doc-calls keys, default all) and :tables (table option
  sets, default [nil])."
  [{:id "simple1" :file "simple1.pdf"}
   {:id "simple3" :file "simple3.pdf"}
   {:id "pagelabels-pages" :file "contrib/pagelabels.pdf" :opts {:pages [2 4]}}
   {:id "pagelabels-max" :file "contrib/pagelabels.pdf" :opts {:max-pages 2}}
   {:id "empty-page" :file "simple2.pdf"}
   {:id "encrypted-empty" :file "encryption/encrypted_doc_no_id.pdf"}
   {:id "arabic" :path "tests/jvm-golden/fixtures/arabic.pdf"}
   {:id "arabic-bidi" :path "tests/jvm-golden/fixtures/arabic.pdf" :opts {:bidi true}}
   {:id "nics" :file "pdfplumber/nics-background-checks-2015-11.pdf"
    :tables [nil
             {:vertical-strategy "text" :horizontal-strategy "text"}
             {:snap-tolerance 5 :join-x-tolerance 1.5 :intersection-y-tolerance 2}
             {:crop [0 100 400 500]}
             {:first-page-crop [50 50 300 300] :crop [0 0 10 10]}
             {:vertical-strategy "explicit" :horizontal-strategy "lines"
              :explicit-vertical-lines [50 150 250 350 450]}
             {:max-pages 1}]}
   {:id "mcid" :file "pdfplumber/mcid_example.pdf"}
   {:id "annotations" :file "pdfplumber/annotations.pdf"}
   {:id "vertical" :file "contrib/issue-449-vertical.pdf"}
   {:id "vertical-detect" :file "contrib/issue-449-vertical.pdf"
    :opts {:layout {:detect-vertical true}}}
   {:id "horizontal" :file "contrib/issue-449-horizontal.pdf"}
   {:id "jo" :file "jo.pdf"}
   {:id "jo-detect" :file "jo.pdf" :opts {:layout {:detect-vertical true}}}
   {:id "pagelabels" :file "contrib/pagelabels.pdf"}
   {:id "colors" :file "test_pattern_colors.pdf"}
   {:id "font-size" :file "font-size-test.pdf"}
   {:id "layout-params" :file "simple1.pdf"
    :opts {:layout {:char-margin 4.0 :line-margin 0.3 :word-margin 0.2 :line-overlap 0.4
                    :boxes-flow 0.2 :all-texts true}}}
   {:id "no-caching" :file "simple3.pdf" :opts {:caching false :max-pages 1}}
   {:id "rc4-40" :file "encryption/rc4-40.pdf" :opts {:password "foo"}}
   {:id "aes-256" :file "encryption/aes-256.pdf" :opts {:password "foo"}}
   {:id "wrong-password" :file "encryption/rc4-40.pdf" :opts {:password "bar"}}
   {:id "corrupted" :file "zen_of_python_corrupted.pdf"}
   {:id "missing-file" :file "does-not-exist.pdf"}
   {:id "page-zero" :file "simple1.pdf" :opts {:pages [0]}}
   {:id "bad-strategy" :file "simple1.pdf" :calls [] :tables [{:vertical-strategy "bogus"}]}
   {:id "negative-tolerance" :file "simple1.pdf" :calls [] :tables [{:snap-tolerance -1}]}])

(defn run-case [{:keys [id file path opts calls tables] :or {tables [nil]}}]
  (let [path (or path (str fixtures file))
        calls (if calls (select-keys doc-calls calls) doc-calls)]
    {:case    {:id id :path path :opts opts :tables tables}
     :results
     (try
       (with-open [d (pdf/open path opts)]
         (merge
          (into {} (for [[k f] calls] [k (attempt #(f d))]))
          {:tables-by-opts
           (mapv (fn [t] (into {} (for [[k f] (table-calls t)] [k (attempt #(f d))]))) tables)}))
       (catch Exception e {:open (error-data e)}))}))

(defn write-case [out-dir result]
  (let [f (io/file out-dir (str (get-in result [:case :id]) ".edn.gz"))]
    (with-open [w (io/writer (GZIPOutputStream. (io/output-stream f)))]
      (binding [*print-length* nil *print-level* nil *out* w]
        (prn result)))
    f))

(defn read-case [f]
  (with-open [r (java.io.PushbackReader.
                 (io/reader (java.util.zip.GZIPInputStream. (io/input-stream (io/file f)))))]
    (edn/read r)))

(defn -main [& [out-dir]]
  (let [out-dir (or out-dir "tests/jvm-golden/corpus")]
    (.mkdirs (io/file out-dir))
    (doseq [c cases]
      (let [r (run-case c)]
        ;; Every case must read back to an equal value, or the corpus is useless.
        (assert (= r (read-case (write-case out-dir r))) (str "round trip failed: " (:id c)))
        (println (:id c) (if (:open (:results r)) "(open error)" "ok"))))))
