(ns sa.ingenious.pdf-budget-test
  "JVM bytes the Clojure layer allocates per character, against a budget. Bytes, unlike time,
  do not depend on machine load, so these checks are stable."
  (:require [clojure.test :refer [deftest is]]
            [sa.ingenious.corpus :as corpus]
            [sa.ingenious.pdf :as pdf])
  (:import [java.lang.management ManagementFactory]))

(def ^:private document (str corpus/repo "/crates/core/tests/fixtures/nonfree/i1040nr.pdf"))

(def ^:private threads
  ^com.sun.management.ThreadMXBean (ManagementFactory/getThreadMXBean))

(defn- bytes-per-char
  "The median JVM bytes `f` allocates on this thread per character of the document."
  [f]
  (with-open [doc (pdf/open document)]
    (let [chars (transduce (map (comp count :chars)) + 0 (pdf/pages doc))
          run   (fn []
                  (let [before (.getCurrentThreadAllocatedBytes threads)]
                    (f doc)
                    (- (.getCurrentThreadAllocatedBytes threads) before)))]
      (dotimes [_ 3] (run))
      (/ (double (nth (sort (repeatedly 7 run)) 3)) chars))))

(defn- within [budget measured label]
  (println (format "budget: %-26s %7.1f B/char (budget %d)" label measured budget))
  (is (<= measured budget) label))

(deftest the-clojure-layer-stays-within-its-allocation-budget
  (within 75 (bytes-per-char #(reduce (fn [n _] (inc n)) 0 (pdf/pages %)))
          "stream pages")
  (within 240 (bytes-per-char #(transduce (comp (mapcat :chars) (map :x0)) + 0.0 (pdf/pages %)))
          "read every char")
  (within 600 (bytes-per-char #(into [] (map pdf/realize) (pdf/pages %)))
          "realize pages")
  (within 2000 (bytes-per-char #(into [] (map pdf/realize) (pdf/raw-pages %)))
          "realize raw pages"))
