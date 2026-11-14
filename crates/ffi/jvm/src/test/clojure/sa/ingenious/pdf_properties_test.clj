(ns sa.ingenious.pdf-properties-test
  "Properties of views and options, checked over generated inputs."
  (:require [clojure.test :refer [deftest is]]
            [clojure.test.check :as tc]
            [clojure.test.check.generators :as gen]
            [clojure.test.check.properties :as prop]
            [sa.ingenious.corpus :as corpus]
            [sa.ingenious.pdf :as pdf]))

(def ^:private nics
  (str corpus/repo "/crates/core/tests/fixtures/pdfplumber/nics-background-checks-2015-11.pdf"))

(def ^:private simple (str corpus/repo "/crates/core/tests/fixtures/simple1.pdf"))

(defn- passes? [result]
  (when-not (:pass? result) (println "property failed:" (pr-str (:shrunk result))))
  (:pass? result))

(deftest views-behave-as-their-realized-maps
  (with-open [d (pdf/open nics)]
    (let [page (first (into [] (pdf/raw-pages d)))
          chars (:chars page)
          lines (:lines page)]
      (is (passes?
           (tc/quick-check
            300
            (prop/for-all [i (gen/choose 0 (dec (count chars)))
                           l (gen/choose 0 (dec (count lines)))
                           ks (gen/vector (gen/elements (vec (keys (nth chars 0)))) 0 6)]
              (let [view (nth chars i)
                    plain (pdf/realize view)
                    line (nth lines l)]
                (and (= view plain) (= plain view)
                     (= (hash view) (hash plain))
                     (= (.hashCode ^Object view) (.hashCode ^Object plain))
                     (= (select-keys view ks) (select-keys plain ks))
                     (= (into {} view) plain)
                     (= (reduce-kv assoc {} view) plain)
                     (= line (pdf/realize line))
                     (= (hash line) (hash (pdf/realize line))))))))))))

(def ^:private gen-value
  (gen/one-of [gen/small-integer gen/large-integer gen/double gen/boolean gen/string-ascii
               gen/keyword (gen/return nil) (gen/vector gen/small-integer 0 4)
               (gen/vector gen/double 4)]))

(def ^:private option-keys
  [:password :pages :max-pages :caching :bidi :bogus])

(def ^:private layout-keys
  [:line-overlap :char-margin :line-margin :word-margin :boxes-flow :detect-vertical :all-texts
   :sideways])

(def ^:private gen-options
  (gen/let [top (gen/map (gen/elements option-keys) gen-value {:max-elements 4})
            layout (gen/map (gen/elements layout-keys) gen-value {:max-elements 4})
            with-layout gen/boolean]
    (cond-> top with-layout (assoc :layout layout))))

(deftest any-options-open-or-fail-as-anomalies
  ;; Whatever the options, opening either works or throws ex-info naming an error: never a raw
  ;; Java or native failure.
  (is (passes?
       (tc/quick-check
        300
        (prop/for-all [opts gen-options]
          (try
            (with-open [d (pdf/open simple opts)] (string? (pdf/text d)))
            (catch clojure.lang.ExceptionInfo e
              (and (keyword? (::pdf/error (ex-data e)))
                   (keyword? (:cognitect.anomalies/category (ex-data e)))))))))))
